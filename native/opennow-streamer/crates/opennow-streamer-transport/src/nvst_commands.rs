use super::{NvstStopCause, TransportError, UdpReceiverCommand};
use crate::nvst_input::{native_input_type_is_motion, native_input_types};
use std::collections::VecDeque;
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex};

pub(super) const COMMAND_MESSAGES: usize = 256;
pub(super) const COMMAND_BYTES: usize = 64 * 1024;
pub(super) const COMMANDS_PER_ITERATION: usize = 32;

struct QueuedCommand {
    command: UdpReceiverCommand,
    bytes: usize,
    motion: bool,
}

#[derive(Default)]
struct Mailbox {
    pending: VecDeque<QueuedCommand>,
    bytes: usize,
    stop: Option<NvstStopCause>,
    closed: bool,
    dropped_motion: u64,
}

#[derive(Clone)]
pub(super) struct CommandSender(Arc<Mutex<Mailbox>>);

pub(super) struct CommandReceiver(Arc<Mutex<Mailbox>>);

pub(super) fn command_queue() -> (CommandSender, CommandReceiver) {
    let mailbox = Arc::new(Mutex::new(Mailbox::default()));
    (CommandSender(mailbox.clone()), CommandReceiver(mailbox))
}

impl Mailbox {
    fn stop(&mut self, cause: NvstStopCause) {
        self.stop = Some(cause);
        self.closed = true;
        self.pending.clear();
        self.bytes = 0;
    }

    fn drop_motion(&mut self, command: UdpReceiverCommand) {
        if let UdpReceiverCommand::SendInput {
            reply: Some(reply), ..
        } = command
        {
            let _ = reply.send(Ok(()));
        }
        self.dropped_motion = self.dropped_motion.saturating_add(1);
        if self.dropped_motion % 1024 == 1 {
            opennow_streamer_protocol::log::log_async(
                "WARN",
                "nvst-input",
                &format!(
                    "admission dropped_motion={} pending_messages={} pending_bytes={}",
                    self.dropped_motion,
                    self.pending.len(),
                    self.bytes
                ),
            );
        }
    }
}

impl CommandSender {
    pub(super) fn send(&self, command: UdpReceiverCommand) -> Result<(), TransportError> {
        let bytes = match &command {
            UdpReceiverCommand::SendInput { bytes, .. } => bytes.len(),
            UdpReceiverCommand::SendText { text, .. } => text.as_str().len(),
            _ => 0,
        };
        let motion = bytes <= COMMAND_BYTES
            && matches!(&command,
                UdpReceiverCommand::SendInput { bytes, .. } if native_input_types(bytes).is_ok_and(|types| !types.is_empty() && types.into_iter().all(native_input_type_is_motion))
            );
        let mut mailbox = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if mailbox.closed {
            return Err(TransportError::Closed);
        }
        if let UdpReceiverCommand::Stop(cause) = command {
            mailbox.stop(cause);
            return Ok(());
        }
        while mailbox.pending.len() >= COMMAND_MESSAGES
            || mailbox.bytes.saturating_add(bytes) > COMMAND_BYTES
        {
            if motion {
                mailbox.drop_motion(command);
                return Ok(());
            }
            if let Some(index) = mailbox.pending.iter().position(|queued| queued.motion) {
                let evicted = mailbox.pending.remove(index).expect("motion index");
                mailbox.bytes -= evicted.bytes;
                mailbox.drop_motion(evicted.command);
                continue;
            }
            if !matches!(command, UdpReceiverCommand::SendText { .. }) {
                mailbox.stop(NvstStopCause::InputDeliveryFailed);
            }
            return Err(TransportError::Closed);
        }
        mailbox.bytes += bytes;
        mailbox.pending.push_back(QueuedCommand {
            command,
            bytes,
            motion,
        });
        Ok(())
    }
}

impl CommandReceiver {
    pub(super) fn try_recv(&self) -> Result<UdpReceiverCommand, TryRecvError> {
        let mut mailbox = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cause) = mailbox.stop.take() {
            return Ok(UdpReceiverCommand::Stop(cause));
        }
        if let Some(queued) = mailbox.pending.pop_front() {
            mailbox.bytes -= queued.bytes;
            return Ok(queued.command);
        }
        if mailbox.closed || Arc::strong_count(&self.0) == 1 {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }
}

impl Drop for CommandReceiver {
    fn drop(&mut self) {
        let mut mailbox = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        mailbox.closed = true;
        mailbox.pending.clear();
        mailbox.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(input_type: u32, bytes: usize) -> UdpReceiverCommand {
        let mut packet = vec![0; bytes];
        packet[..4].copy_from_slice(&input_type.to_le_bytes());
        UdpReceiverCommand::SendInput {
            bytes: packet,
            reply: None,
        }
    }

    #[test]
    fn motion_pressure_is_bounded_and_discrete_releases_keep_fifo_order() {
        let (sender, receiver) = command_queue();
        sender.send(input(3, 18)).unwrap();
        for _ in 0..COMMAND_MESSAGES * 4 {
            sender.send(input(7, 22)).unwrap();
        }
        sender.send(input(4, 18)).unwrap();
        let mailbox = sender.0.lock().unwrap();
        assert_eq!(mailbox.pending.len(), COMMAND_MESSAGES);
        assert!(mailbox.bytes <= COMMAND_BYTES);
        assert!(mailbox.dropped_motion > 0);
        drop(mailbox);
        let mut discrete = Vec::new();
        while let Ok(UdpReceiverCommand::SendInput { bytes, .. }) = receiver.try_recv() {
            let kind = u32::from_le_bytes(bytes[..4].try_into().unwrap());
            if kind != 7 {
                discrete.push(kind);
            }
        }
        assert_eq!(discrete, [3, 4]);
        assert_eq!(sender.0.lock().unwrap().bytes, 0);
    }

    #[test]
    fn byte_pressure_settles_discrete_delivery_and_releases_text_reservation() {
        let (sender, receiver) = command_queue();
        let slot = opennow_streamer_protocol::text_input::TextInputSlot::default();
        sender
            .send(UdpReceiverCommand::SendText {
                text: slot.submit(&vec![b'x'; COMMAND_BYTES]).unwrap(),
                timestamp_us: 0,
            })
            .unwrap();
        assert_eq!(sender.0.lock().unwrap().bytes, COMMAND_BYTES);
        assert!(sender.send(input(4, 18)).is_err());
        assert!(matches!(
            receiver.try_recv(),
            Ok(UdpReceiverCommand::Stop(NvstStopCause::InputDeliveryFailed))
        ));
        assert!(slot.submit(b"new session").is_ok());
        assert!(sender.0.lock().unwrap().pending.is_empty());
    }

    #[test]
    fn rejected_paste_does_not_abort_discrete_input_and_drop_disconnects() {
        let (sender, receiver) = command_queue();
        for _ in 0..COMMAND_MESSAGES {
            sender.send(input(3, 18)).unwrap();
        }
        let slot = opennow_streamer_protocol::text_input::TextInputSlot::default();
        assert!(
            sender
                .send(UdpReceiverCommand::SendText {
                    text: slot.submit(b"paste").unwrap(),
                    timestamp_us: 0
                })
                .is_err()
        );
        assert!(!sender.0.lock().unwrap().closed);
        assert!(slot.submit(b"retry").is_ok());
        drop(sender);
        for _ in 0..COMMAND_MESSAGES {
            assert!(receiver.try_recv().is_ok());
        }
        assert!(matches!(
            receiver.try_recv(),
            Err(TryRecvError::Disconnected)
        ));
    }
}
