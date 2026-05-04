//! RustDesk variable-length framing codec and protobuf helpers.

mod codec;

pub use codec::{FrameCodec, write_frame, read_frame, encode_frame};
