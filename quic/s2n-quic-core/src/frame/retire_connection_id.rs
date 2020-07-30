use crate::varint::VarInt;

//= https://tools.ietf.org/id/draft-ietf-quic-transport-29.txt#19.16
//# An endpoint sends a RETIRE_CONNECTION_ID frame (type=0x19) to
//# indicate that it will no longer use a connection ID that was issued
//# by its peer.

macro_rules! retire_connection_id_tag {
    () => {
        0x19u8
    };
}

//= https://tools.ietf.org/id/draft-ietf-quic-transport-29.txt#19.16
//# The RETIRE_CONNECTION_ID frame is shown in Figure 39.
//#
//# RETIRE_CONNECTION_ID Frame {
//#   Type (i) = 0x19,
//#   Sequence Number (i),
//# }
//#
//#              Figure 39: RETIRE_CONNECTION_ID Frame Format
//#
//# RETIRE_CONNECTION_ID frames contain the following fields:
//#
//# Sequence Number:  The sequence number of the connection ID being
//#    retired; see Section 5.1.2.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RetireConnectionID {
    pub sequence_number: VarInt,
}

impl RetireConnectionID {
    pub const fn tag(self) -> u8 {
        retire_connection_id_tag!()
    }
}

simple_frame_codec!(
    RetireConnectionID { sequence_number },
    retire_connection_id_tag!()
);