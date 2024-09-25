use crate::wallet::MemWallet;
use crate::Client;
use sn_client::transfers::{MainPubkey, NanoTokens};
use sn_transfers::{SpendReason, Transfer};

use sn_transfers::UniquePubkey;
use std::collections::BTreeSet;

#[derive(Debug, thiserror::Error)]
pub enum SendSpendsError {
    /// The cashnotes that were attempted to be spent have already been spent to another address
    #[error("Double spend attempted with cashnotes: {0:?}")]
    DoubleSpendAttemptedForCashNotes(BTreeSet<UniquePubkey>),
    /// A general error when a transfer fails
    #[error("Failed to send tokens due to {0}")]
    CouldNotSendMoney(String),
}

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("Failed to send tokens due to {0}")]
    CouldNotSendMoney(String),
    #[error("Wallet error: {0:?}")]
    WalletError(#[from] crate::wallet::error::WalletError),
    #[error("Network error: {0:?}")]
    NetworkError(#[from] sn_networking::NetworkError),
}

#[derive(Debug, thiserror::Error)]
pub enum CashNoteError {
    #[error("CashNote was already spent.")]
    AlreadySpent,
    #[error("Failed to get spend: {0:?}")]
    FailedToGetSpend(String),
}

use libp2p::{
    futures::future::join_all,
    kad::{Quorum, Record},
    PeerId,
};
use sn_client::transfers::{HotWallet, SignedSpend};
use sn_networking::{
    GetRecordCfg, GetRecordError, Network, NetworkError, PutRecordCfg, VerificationKind,
};
use sn_protocol::{
    storage::{try_serialize_record, RecordKind, RetryStrategy, SpendAddress},
    NetworkAddress, PrettyPrintRecordKey,
};
use sn_transfers::Payment;
use xor_name::XorName;

use crate::VERIFY_STORE;
use sn_transfers::CashNote;
use std::collections::HashSet;

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("CashNote amount unexpected: {0}")]
    CashNoteAmountUnexpected(String),
    #[error("CashNote has no parent spends.")]
    CashNoteHasNoParentSpends,
    #[error("Wallet error occurred during sending of transfer.")]
    WalletError(#[from] crate::wallet::error::WalletError),
    #[error("Encountered transfer error during sending.")]
    TransferError(#[from] sn_transfers::TransferError),
    #[error("Spends error: {0:?}")]
    SpendsError(#[from] SendSpendsError),
}

#[derive(Debug, thiserror::Error)]
pub enum ReceiveError {
    #[error("Could not deserialize `Transfer`.")]
    TransferDeserializationFailed,
    #[error("Transfer error occurred during receiving.")]
    TransferError(#[from] TransferError),
}

// Hide these from the docs.
#[doc(hidden)]
impl Client {
    /// Send spend requests to the network.
    pub async fn send_spends(
        &self,
        spend_requests: impl Iterator<Item = &SignedSpend>,
    ) -> Result<(), SendSpendsError> {
        let mut tasks = Vec::new();

        // send spends to the network in parralel
        for spend_request in spend_requests {
            tracing::debug!(
                "sending spend request to the network: {:?}: {spend_request:#?}",
                spend_request.unique_pubkey()
            );

            let the_task = async move {
                let cash_note_key = spend_request.unique_pubkey();
                let result = store_spend(self.network.clone(), spend_request.clone()).await;

                (cash_note_key, result)
            };
            tasks.push(the_task);
        }

        // wait for all the tasks to complete and gather the errors
        let mut errors = Vec::new();
        let mut double_spent_keys = BTreeSet::new();
        for (spend_key, spend_attempt_result) in join_all(tasks).await {
            match spend_attempt_result {
                Err(sn_networking::NetworkError::GetRecordError(
                    GetRecordError::RecordDoesNotMatch(_),
                ))
                | Err(sn_networking::NetworkError::GetRecordError(GetRecordError::SplitRecord {
                    ..
                })) => {
                    tracing::warn!(
                        "Double spend detected while trying to spend: {:?}",
                        spend_key
                    );
                    double_spent_keys.insert(*spend_key);
                }
                Err(e) => {
                    tracing::warn!(
                        "Spend request errored out when sent to the network {spend_key:?}: {e}"
                    );
                    errors.push((spend_key, e));
                }
                Ok(()) => {
                    tracing::trace!(
                        "Spend request was successfully sent to the network: {spend_key:?}"
                    );
                }
            }
        }

        // report errors accordingly
        // double spend errors in priority as they should be dealt with by the wallet
        if !double_spent_keys.is_empty() {
            return Err(SendSpendsError::DoubleSpendAttemptedForCashNotes(
                double_spent_keys,
            ));
        }
        if !errors.is_empty() {
            let mut err_report = "Failed to send spend requests to the network:".to_string();
            for (spend_key, e) in &errors {
                tracing::warn!("Failed to send spend request to the network: {spend_key:?}: {e}");
                err_report.push_str(&format!("{spend_key:?}: {e}"));
            }
            return Err(SendSpendsError::CouldNotSendMoney(err_report));
        }

        Ok(())
    }

    /// Resend failed transactions. This can optionally verify the store has been successful.
    /// This will attempt to GET the cash_note from the network.
    pub(super) async fn resend_pending_transactions(&mut self, wallet: &mut HotWallet) {
        if wallet.unconfirmed_spend_requests().is_empty() {
            return;
        }

        if self
            .send_spends(wallet.unconfirmed_spend_requests().iter())
            .await
            .is_ok()
        {
            wallet.clear_confirmed_spend_requests();
        }
    }

    /// Deposits all valid `CashNotes` from a transfer into a wallet.
    pub(super) async fn receive_transfer(
        &self,
        transfer: Transfer,
        wallet: &mut MemWallet,
    ) -> Result<(), TransferError> {
        let cash_note_redemptions = wallet
            .unwrap_transfer(&transfer)
            .map_err(TransferError::WalletError)?;

        let cash_notes = self
            .network
            .verify_cash_notes_redemptions(wallet.address(), &cash_note_redemptions)
            .await?;

        for cash_note in cash_notes {
            match self.verify_if_cash_note_is_valid(&cash_note).await {
                Ok(_) => wallet.deposit_cash_note(cash_note)?,
                Err(e) => {
                    tracing::warn!("Error verifying CashNote: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Verify if a `CashNote` is unspent.
    pub(super) async fn verify_if_cash_note_is_valid(
        &self,
        cash_note: &CashNote,
    ) -> Result<(), CashNoteError> {
        let pk = cash_note.unique_pubkey();
        let addr = SpendAddress::from_unique_pubkey(&pk);

        match self.network.get_spend(addr).await {
            // if we get a RecordNotFound, it means the CashNote is not spent, which is good
            Err(NetworkError::GetRecordError(GetRecordError::RecordNotFound)) => Ok(()),
            // if we get a spend, it means the CashNote is already spent
            Ok(_) => Err(CashNoteError::AlreadySpent),
            // report all other errors
            Err(e) => Err(CashNoteError::FailedToGetSpend(format!("{e}"))),
        }
    }

    /// Returns the most recent cached Payment for a provided NetworkAddress. This function does not check if the
    /// quote has expired or not. Use get_non_expired_payment_for_addr if you want to get a non expired one.
    ///
    /// If multiple payments have been made to the same address, then we pick the last one as it is the most recent.
    pub fn get_recent_payment_for_addr(
        &self,
        xor_name: &XorName,
        wallet: &mut HotWallet,
    ) -> Result<(Payment, PeerId), sn_transfers::WalletError> {
        let payment_detail = wallet.api().get_recent_payment(xor_name)?;

        let payment = payment_detail.to_payment();
        let peer_id = PeerId::from_bytes(&payment_detail.peer_id_bytes)
            .expect("payment detail should have a valid peer id");

        Ok((payment, peer_id))
    }

    /// Creates a `Transfer` that can be received by the receiver.
    /// Once received, it will be turned into a `CashNote` that the receiver can spend.
    pub async fn send(
        &mut self,
        to: MainPubkey,
        amount_in_nano: NanoTokens,
        reason: Option<SpendReason>,
        wallet: &mut MemWallet,
    ) -> Result<Transfer, SendError> {
        let signed_transaction =
            wallet.create_signed_transaction(vec![(amount_in_nano, to)], reason)?;

        // return the first CashNote (assuming there is only one because we only sent to one recipient)
        let cash_note_for_recipient = match &signed_transaction.output_cashnotes[..] {
            [cash_note] => Ok(cash_note),
            [_multiple, ..] => Err(SendError::CashNoteAmountUnexpected(
                "Got multiple, expected 1.".into(),
            )),
            [] => Err(SendError::CashNoteAmountUnexpected(
                "Got 0, expected 1.".into(),
            )),
        }?;

        let transfer = Transfer::transfer_from_cash_note(cash_note_for_recipient)
            .map_err(SendError::TransferError)?;

        self.send_spends(signed_transaction.spends.iter()).await?;

        wallet.process_signed_transaction(signed_transaction.clone());

        for spend in &signed_transaction.spends {
            wallet.add_pending_spend(spend.clone());
        }

        Ok(transfer)
    }

    /// Receive a `CashNoteRedemption` through a transfer message.
    pub async fn receive(
        &self,
        transfer_hex: &str,
        wallet: &mut MemWallet,
    ) -> Result<(), ReceiveError> {
        let transfer = Transfer::from_hex(transfer_hex)
            .map_err(|_| ReceiveError::TransferDeserializationFailed)?;
        self.receive_transfer(transfer, wallet).await?;
        Ok(())
    }
}

/// Send a `SpendCashNote` request to the network.
async fn store_spend(network: Network, spend: SignedSpend) -> Result<(), NetworkError> {
    let unique_pubkey = *spend.unique_pubkey();
    let cash_note_addr = SpendAddress::from_unique_pubkey(&unique_pubkey);
    let network_address = NetworkAddress::from_spend_address(cash_note_addr);

    let key = network_address.to_record_key();
    let pretty_key = PrettyPrintRecordKey::from(&key);
    tracing::trace!("Sending spend {unique_pubkey:?} to the network via put_record, with addr of {cash_note_addr:?} - {pretty_key:?}");
    let record_kind = RecordKind::Spend;
    let record = Record {
        key,
        value: try_serialize_record(&[spend], record_kind)?.to_vec(),
        publisher: None,
        expires: None,
    };

    let (record_to_verify, expected_holders) = if VERIFY_STORE {
        let expected_holders: HashSet<_> = network
            .get_closest_peers(&network_address, true)
            .await?
            .iter()
            .cloned()
            .collect();
        (Some(record.clone()), expected_holders)
    } else {
        (None, Default::default())
    };

    // When there is retry on Put side, no need to have a retry on Get
    let verification_cfg = GetRecordCfg {
        get_quorum: Quorum::Majority,
        retry_strategy: None,
        target_record: record_to_verify,
        expected_holders,
        is_register: false,
    };
    let put_cfg = PutRecordCfg {
        put_quorum: Quorum::Majority,
        retry_strategy: Some(RetryStrategy::Persistent),
        use_put_record_to: None,
        verification: Some((VerificationKind::Network, verification_cfg)),
    };
    network.put_record(record, &put_cfg).await
}
