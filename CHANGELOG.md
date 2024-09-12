# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

*When editing this file, please respect a line length of 100.*

## 2024-09-09

### Network

#### Added

- More logging for storage errors and setting the responsible range.

#### Changed

- The node's store cost calculation has had various updates:
    + The minimum and maximum were previously set to 10 and infinity. They've now been updated to 1
      and 1 million, respectively.
    + We are now using a sigmoid curve, rather than a linear curve, as the base curve. The previous
      curve only grew steep when the storage capacity was 40 to 60 percent.
    + The overall calculation is simplified.
- We expect the updates to the store cost calculation to prevent 'lottery' payments, where one node
  would have abnormally high earnings.
- The network version string, which is used when both nodes and clients connect to the network, now
  uses the version number from the `sn_protocol` crate rather than `sn_networking`. This is a
  breaking change in `sn_networking`.
- External address management is improved. Before, if anyone observed us at a certain public
  IP+port, we would trust that and add it if it matches our local port. Now, we’re keeping track and
  making sure we only have a single external address that we set when we’ve been observed as that
  address a certain amount of times (3 by default). It should even handle cases where our IP changes
  because of (mobile) roaming.
- The `Spend` network data type has been refactored to make it lighter and simpler.
- The entire transaction system has been redesigned; the code size and complexity have been reduced
  by an order of magnitude.
- In addition, almost 10 types were removed from the transaction code, further reducing the
  complexity.
- The internals of the `Transfer` and `CashNote` types have been reworked.
- The replication range has been reduced, which in turn reduces the base traffic for replication.

### Client

#### Fixed

- Registers are fetched and merged correctly. 

### Launchpad

#### Added

- A connection mode feature enables users to select whether they want their nodes to connect to the
  network using automatic NAT detection, upnp, home network, or custom port mappings in their
  connection. Previously, the launchpad used NAT detection on the user’s behalf. By providing the
  ability to explore more connection modes, hopefully this will get more users connected.

#### Changed

- On the drive selection dialog, drives to which the user does not have read or write access are
  marked as such.

### Documentation

#### Added

- A README was provided for the `sn_registers` crate. It intends to give a comprehensive
  understanding of the register data type and how it can be used by developers.

#### Changed

- Provided more information on connecting to the network using the four keys related to funds, fees
  and royalties.

## 2024-09-02

### Launchpad

#### Fixed

- Some users encountered an error when the launchpad started, related to the storage mountpoint not
  being set. We fix the error by providing default values for the mountpoint settings when the
  `app_data.json` file doesn't exist (fresh install). In the case where it does exist, we validate
  the contents.

## 2024-08-27

### Network

#### Added

- The node will now report its bandwidth usage through the metrics endpoint.
- The metrics server has a new `/metadata` path which will provide static information about the node,
  including peer ID and version.
- The metrics server exposes more metrics on store cost derivation. These include relevant record
  count and number of payments received.
- The metrics server exposes metrics related to bad node detection.
- Test to confirm main key can’t verify signature signed by child key.
- Avoid excessively high quotes by pruning records that are not relevant.

#### Changed

- Bad node detection and bootstrap intervals have been increased. This should reduce the number
  of messages being sent.
- The spend parent verification strategy was refactored to be more aligned with the public
  network.
- Nodes now prioritize local work over new work from the network, which reduces memory footprint.
- Multiple GET queries to the same address are now de-duplicated and will result in a single query
  being processed.
- Improve efficiency of command handling and the record store cache.
- A parent spend is now trusted with a majority of close group nodes, rather than all of them. This
  increases the chance of the spend being stored successfully when some percentage of nodes are slow
  to respond.

#### Fixed

- The amount of bytes a home node could send and receive per relay connection is increased. This
  solves a problem where transmission of data is interrupted, causing home nodes to malfunction.
- Fetching the network contacts now times out and retries. Previously we would wait for an excessive
  amount of time, which could cause the node to hang during start up.
- If a node has been shunned, we inform that node before blocking all communication to it.
- The current wallet balance metric is updated more frequently and will now reflect the correct
  state.
- Avoid burnt spend during forwarding by correctly handling repeated CashNotes and confirmed spends.
- Fix logging for CashNote and confirmed spend disk ops
- Check whether a CashNote has already been received to avoid duplicate CashNotes in the wallet.

### Node Manager

#### Added

- The `local run` command supports `--metrics-port`, `--node-port` and `--rpc-port` arguments.
- The `start` command waits for the node to connect to the network before attempting to start the
  next node. If it takes more than 300 seconds to connect, we consider that a failure and move to the
  next node. The `--connection-timeout` argument can be used to vary the timeout. If you prefer the
  old behaviour, you can use the `--interval` argument, which will continue to apply a static,
  time-based interval.

#### Changed

- On an upgrade, the node registry is saved after each node is processed, as opposed to waiting
  until the end. This means if there is an unexpected failure, the registry will have the
  information about which nodes have already been upgraded.

### Launchpad

#### Added

- The user can choose a different drive for the node's data directory.
- New sections in the UI: `Options` and `Help`.
- A navigation bar has been added with `Status`, `Options` and `Help` sections.
- The node's logs can be viewed from the `Options` section.

#### Changed

- Increased spacing for title and paragraphs.
- Increased spacing on footer.
- Increased spacing on box titles.
- Moved `Discord Username` from the top title into the `Device Status` section.
- Made the general layout of `Device Status` more compact.

### Client

#### Added

- The `safe files download` command now displays duration per file.

#### Changed

- Adjust the put and get configuration scheme to align the client with a more realistic network
  which would have some percentage of slow nodes.
- Improved spend logging to help debug the upload process.

#### Fixed

- Avoid a corrupt wallet by terminating the payment process during an unrecoverable error.

## 2024-07-25

### Network

#### Added

- Protection against an attack allowing bad nodes or clients to shadow a spend (make it disappear)
  through spamming.
- Nodes allow more relayed connections through them. Also, home nodes will relay through 4 nodes
  instead of 2. Without these changes, relays were denying new connections to home nodes, making them
  difficult to reach.
- Auditor tracks forwarded payments using the default key. 
- Auditor tracks burnt spend attempts and only credits them once.
- Auditor collects balance of UTXOs.
- Added different attack types to the spend simulation test to ensure spend validation is solid.
- Bad nodes and nodes with a mismatched protocol are now added to a block list. This reduces the
  chance of a network interference and the impact of a bad node in the network.
- The introduction of a record-store cache has significantly reduced the node's disk IO. As a side
  effect, the CPU does less work, and performance improves. RAM usage has increased by around 25MB per
  node, but we view this as a reasonable trade off.

#### Changed

- For the time being, hole punching has been removed. It was causing handshake time outs, resulting
  in home nodes being less stable. It will be re-enabled in the future.
- Force connection closure if a peer is using a different protocol.
- Reserve trace level logs for tracking event statistics. Now you can use `SN_LOG=v` to get more
  relevant logs without being overwhelmed by event handling stats.
- Chunk verification is now probabilistic, which should reduce messaging. In combination with
  replication messages also being reduced, this should result in a bandwidth usage reduction of
  ~20%.

#### Fixed

- During payment forwarding, CashNotes are removed from disk and confirmed spends are stored to
  disk. This is necessary for resolving burnt spend attempts for forwarded payments.
- Fix a bug where the auditor was not storing data to disk because of a missing directory.
- Bootstrap peers are not added as relay candidates as we do not want to overwhelm them.

### Client

#### Added

- Basic global documentation for the `sn_client` crate.
- Option to encrypt the wallet private key with a password, in a file called
  `main_secret_key.encrypted`, inside the wallet directory.
- Option to load a wallet from an encrypted secret-key file using a password.
- The `wallet create` command provides a `--password` argument to encrypt the wallet.
- The `wallet create` command provides a `--no-password` argument skip encryption.
- The `wallet create` command provides a `--no-replace` argument to suppress a prompt to replace an
  existing wallet.
- The `wallet create` command provides a `--key` argument to create a wallet from a hex-encoded
  private key.
- The `wallet create` command provides a `--derivation` argument to set a derivation passphrase to
  be used with the mnemonic to create a new private key.
- A new `wallet encrypt` command encrypts an existing wallet.

#### Changed

- The `wallet address` command no longer creates a new wallet if no wallet exists.
- The `wallet create` command creates a wallet using the account mnemonic instead of requiring a
  hex-encoded secret key.
- The `wallet create` `--key` and `--derivation` arguments are mutually exclusive.

### Launchpad

#### Fixed

- The `Total Nanos Earned` stat no longer resets on restart.

### RPC Client

#### Added

- A `--version` argument shows the binary version

### Other

#### Added

- Native Apple Silicon (M-series) binaries have been added to our releases, meaning M-series Mac
  users do not have to rely on running Intel binaries with Rosetta.

## 2024-07-10

### Network

#### Added

- The node exposes more metrics, including its uptime, number of connected peers, number of peers in
  the routing table, and the number of open connections. These will help us more effectively
  diagnose user issues.

#### Changed

- Communication between node and client is strictly limited through synchronised public keys. The
  current beta network allows the node and client to use different public keys, resulting in
  undefined behaviour and performance issues. This change mitigates some of those issues and we also
  expect it to prevent other double spend issues.
- Reduced base traffic for nodes, resulting in better upload performance. This will result in better
  distribution of nanos, meaning users with a smaller number of nodes will be expected to receive
  nanos more often.

#### Fixed

- In the case where a client retries a failed upload, they would re-send their payment. In a rare
  circumstance, the node would forward this reward for a second time too. This is fixed on the node.
- Nodes are prevented from double spending under rare circumstances.
- ARM builds are no longer prevented from connecting to the network.

### Node Manager

#### Added

- Global `--debug` and `--trace` arguments are provided. These will output debugging and trace-level
  logging, respectively, direct to stderr.

#### Changed

- The mechanism used by the node manager to refresh its state is significantly changed to address
  issues that caused commands to hang for long periods of time. Now, when using commands like
  `start`, `stop`, and `reset`, users should no longer experience the commands taking excessively
  long to complete.
- The `nat-detection run` command provides a default list of servers, meaning the `--servers`
  argument is now optional.

### Launchpad

#### Added

- Launchpad and node versions are displayed on the user interface.

#### Changed

- The node manager change for refreshing its state also applies to the launchpad. Users should
  experience improvements in operations that appeared to be hanging but were actually just taking
  an excessive amount of time to complete.

#### Fixed

- The correct primary storage will now be selected on Linux and macOS.
