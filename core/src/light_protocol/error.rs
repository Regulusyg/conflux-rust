// Copyright 2019 Conflux Foundation. All rights reserved.
// Conflux is free software and distributed under GNU General Public License.
// See http://www.gnu.org/licenses/

use crate::{
    message::{Message, MsgId},
    network::{self, NetworkContext, UpdateNodeOperation},
    sync::message::Throttled,
};
use network::node_table::NodeId;
use primitives::ChainIdParams;
use rlp::DecoderError;

error_chain! {
    links {
        Network(network::Error, network::ErrorKind);
    }

    foreign_links {
        Decoder(DecoderError);
    }

    errors {
        AlreadyThrottled(msg_name: &'static str) {
            description("packet already throttled"),
            display("packet already throttled: {:?}", msg_name),
        }

        GenesisMismatch {
            description("Genesis mismatch"),
            display("Genesis mismatch"),
        }

        ChainIdMismatch{ours: ChainIdParams, theirs: ChainIdParams} {
            description("ChainId mismatch"),
            display("ChainId mismatch, ours {:?}, theirs {:?}.", ours, theirs),
        }

        InternalError {
            description("Internal error"),
            display("Internal error"),
        }

        InvalidBloom {
            description("Invalid bloom"),
            display("Invalid bloom"),
        }

        InvalidLedgerProof {
            description("Invalid ledger proof"),
            display("Invalid ledger proof"),
        }

        InvalidMessageFormat {
            description("Invalid message format"),
            display("Invalid message format"),
        }

        InvalidReceipts {
            description("Invalid receipts"),
            display("Invalid receipts"),
        }

        InvalidStateProof {
            description("Invalid state proof"),
            display("Invalid state proof"),
        }

        InvalidStateRoot {
            description("Invalid state root"),
            display("Invalid state root"),
        }

        InvalidTxInfo {
            description("Invalid tx info"),
            display("Invalid tx info"),
        }

        InvalidTxRoot {
            description("Invalid tx root"),
            display("Invalid tx root"),
        }

        InvalidTxSignature {
            description("Invalid tx signature"),
            display("Invalid tx signature"),
        }

        NoResponse {
            description("NoResponse"),
            display("NoResponse"),
        }

        PivotHashMismatch {
            description("Pivot hash mismatch"),
            display("Pivot hash mismatch"),
        }

        SendStatusFailed {
            description("Send status failed"),
            display("Send status failed"),
        }

        Throttled(msg_name: &'static str, response: Throttled) {
            description("packet throttled"),
            display("packet {:?} throttled: {:?}", msg_name, response),
        }

        UnableToProduceProof {
            description("Unable to produce proof"),
            display("Unable to produce proof"),
        }

        UnexpectedMessage {
            description("Unexpected message"),
            display("Unexpected message"),
        }

        UnexpectedPeerType {
            description("Unexpected peer type"),
            display("Unexpected peer type"),
        }

        UnexpectedRequestId {
            description("Unexpected request id"),
            display("Unexpected request id"),
        }

        UnexpectedResponse {
            description("Unexpected response"),
            display("Unexpected response"),
        }

        UnknownMessage {
            description("Unknown message"),
            display("Unknown message"),
        }

        UnknownPeer {
            description("Unknown peer"),
            display("Unknown peer"),
        }

        ValidationFailed {
            description("Validation failed"),
            display("Validation failed"),
        }
    }
}

pub fn handle(io: &dyn NetworkContext, peer: &NodeId, msg_id: MsgId, e: Error) {
    warn!(
        "Error while handling message, peer={}, msg_id={:?}, error={}",
        peer, msg_id, e
    );

    let mut disconnect = true;
    let reason = format!("{}", e.0);
    let mut op = None;

    // NOTE: do not use wildcard; this way, the compiler
    // will help covering all the cases.
    match e.0 {
        ErrorKind::NoResponse
        | ErrorKind::InternalError

        // NOTE: we should be tolerant of non-critical errors,
        // e.g. do not disconnect on requesting non-existing epoch
        | ErrorKind::Msg(_)

        // NOTE: this can happen in normal scenarios
        // where the pivot chain has not converged
        | ErrorKind::PivotHashMismatch

        // NOTE: in order to let other protocols run,
        // we should not disconnect on protocol failure
        | ErrorKind::SendStatusFailed

        // NOTE: if we do not have a confirmed (non-blamed) block
        // with the info needed to produce a state root proof, we
        // should not disconnect the peer
        | ErrorKind::UnableToProduceProof

        // NOTE: to help with backward-compatibility, we
        // should not disconnect on `UnknownMessage`
        | ErrorKind::UnknownMessage => disconnect = false,

        ErrorKind::GenesisMismatch
        | ErrorKind::ChainIdMismatch{..}
        | ErrorKind::UnexpectedMessage
        | ErrorKind::UnexpectedPeerType
        | ErrorKind::UnknownPeer => op = Some(UpdateNodeOperation::Failure),

        ErrorKind::UnexpectedRequestId | ErrorKind::UnexpectedResponse => {
            op = Some(UpdateNodeOperation::Demotion)
        }

        ErrorKind::InvalidBloom
        | ErrorKind::InvalidLedgerProof
        | ErrorKind::InvalidMessageFormat
        | ErrorKind::InvalidReceipts
        | ErrorKind::InvalidStateProof
        | ErrorKind::InvalidStateRoot
        | ErrorKind::InvalidTxInfo
        | ErrorKind::InvalidTxRoot
        | ErrorKind::InvalidTxSignature
        | ErrorKind::ValidationFailed
        | ErrorKind::AlreadyThrottled(_)
        | ErrorKind::Decoder(_) => op = Some(UpdateNodeOperation::Remove),

        ErrorKind::Throttled(_, resp) => {
            disconnect = false;

            if let Err(e) = resp.send(io, peer) {
                error!("failed to send throttled packet: {:?}", e);
                disconnect = true;
            }
        }

        // network errors
        ErrorKind::Network(kind) => match kind {
            network::ErrorKind::SendUnsupportedMessage{..} => {
                unreachable!("This is a bug in protocol version maintenance. {:?}", kind);
            }

            network::ErrorKind::MessageDeprecated{..} => {
                op = Some(UpdateNodeOperation::Failure);
                error!(
                    "Peer sent us a deprecated message {:?}. Either it's a bug \
                    in protocol version maintenance or the peer is malicious.",
                    kind,
                );
            }

            network::ErrorKind::AddressParse
            | network::ErrorKind::AddressResolve(_)
            | network::ErrorKind::Auth
            | network::ErrorKind::BadAddr
            | network::ErrorKind::Disconnect(_)
            | network::ErrorKind::Expired
            | network::ErrorKind::InvalidNodeId
            | network::ErrorKind::Io(_)
            | network::ErrorKind::OversizedPacket
            | network::ErrorKind::Throttling(_) => disconnect = false,

            network::ErrorKind::BadProtocol | network::ErrorKind::Decoder => {
                op = Some(UpdateNodeOperation::Remove)
            }

            network::ErrorKind::SocketIo(_)
            | network::ErrorKind::Msg(_)
            | network::ErrorKind::__Nonexhaustive {} => {
                op = Some(UpdateNodeOperation::Failure)
            }
        },

        ErrorKind::__Nonexhaustive {} => {
            op = Some(UpdateNodeOperation::Failure)
        }
    };

    if disconnect {
        io.disconnect_peer(peer, op, reason.as_str());
    }
}
