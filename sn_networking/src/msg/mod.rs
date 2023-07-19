// Copyright 2023 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use crate::{
    error::{Error, Result},
    MsgResponder, NetworkEvent, SwarmDriver, event,
};

use libp2p::request_response::{self, Message, RequestId};
use sn_protocol::messages::{Request, Response};
use std::collections::HashMap;
use tokio::sync::{oneshot, mpsc};
use tracing::{trace, warn};
impl SwarmDriver {
    /// Forwards `Request` to the upper layers using `Sender<NetworkEvent>`. Sends `Response` to the peers
    pub fn handle_msg(
        // &mut self,
        event: request_response::Event<Request, Response>,
        event_sender: mpsc::Sender<NetworkEvent>,
        pending_requests: &mut HashMap<RequestId, Option<oneshot::Sender<Result<Response>>>>,
    ) -> Result<(), Error> {
        match event {
            request_response::Event::Message { message, .. } => match message {
                Message::Request {
                    request,
                    channel,
                    request_id,
                    ..
                } => {
                    trace!("Received request with id: {request_id:?}, req: {request:?}");
                    Self::send_event(event_sender,NetworkEvent::RequestReceived {
                        req: request,
                        channel: MsgResponder::FromPeer(channel),
                    })
                }
                Message::Response {
                    request_id,
                    response,
                } => {
                    trace!("Got response for id: {request_id:?}, res: {response}.");
                    if let Some(sender) = pending_requests.remove(&request_id) {
                        // The sender will be provided if the caller (Requester) is awaiting for a response
                        // at the call site.
                        // Else the Request was just sent to the peer and the Response was
                        // meant to be handled in another way and is not awaited.
                        match sender {
                            Some(sender) => sender
                                .send(Ok(response))
                                .map_err(|_| Error::InternalMsgChannelDropped)?,
                            None => {
                                // responses that are not awaited at the call site must be handled
                                // separately
                                Self::send_event(event_sender, NetworkEvent::ResponseReceived { res: response });
                            }
                        }
                    } else {
                        warn!("Tried to remove a RequestId from pending_requests which was not inserted in the first place.
                        Use Cmd::SendRequest with sender:None if you want the Response to be fed into the common handle_response function");
                    }
                }
            },
            request_response::Event::OutboundFailure {
                request_id,
                error,
                peer,
            } => {
                debug!("msg out fail");
                if let Some(sender) = pending_requests.remove(&request_id) {
                    match sender {
                        Some(sender) => {
                            sender
                                .send(Err(error.into()))
                                .map_err(|_| Error::InternalMsgChannelDropped)?;
                        }
                        None => {
                            warn!("RequestResponse: OutboundFailure for request_id: {request_id:?} and peer: {peer:?}, with error: {error:?}");
                            return Err(Error::ReceivedResponseDropped(request_id));
                        }
                    }
                } else {
                    warn!("RequestResponse: OutboundFailure for request_id: {request_id:?} and peer: {peer:?}, with error: {error:?}");
                    return Err(Error::ReceivedResponseDropped(request_id));
                }
            }
            request_response::Event::InboundFailure {
                peer,
                request_id,
                error,
            } => {
                warn!("RequestResponse: InboundFailure for request_id: {request_id:?} and peer: {peer:?}, with error: {error:?}");
            }
            request_response::Event::ResponseSent { peer, request_id } => {
                trace!("ResponseSent for request_id: {request_id:?} and peer: {peer:?}");
            }
        }
        Ok(())
    }
}
