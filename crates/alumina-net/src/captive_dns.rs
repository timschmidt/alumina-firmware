//! Bounded DNS replies for the isolated recovery access point.
//!
//! The recovery AP is not a recursive resolver. It answers one ordinary IPv4
//! question with the AP address and returns an empty successful response for
//! every other supported question. Answers use a zero TTL so a client cannot
//! retain the recovery address after moving to another network.

/// Largest DNS-over-UDP packet accepted by the recovery service.
pub const CAPTIVE_DNS_PACKET_BYTES: usize = 512;
/// UDP port used by the DNS service.
pub const CAPTIVE_DNS_PORT: u16 = 53;

const DNS_HEADER_BYTES: usize = 12;
const IPV4_ANSWER_BYTES: usize = 16;
const FLAGS_QUERY_RESPONSE: u16 = 1 << 15;
const FLAGS_OPCODE_MASK: u16 = 0b1111 << 11;
const FLAGS_AUTHORITATIVE: u16 = 1 << 10;
const FLAGS_TRUNCATED: u16 = 1 << 9;
const FLAGS_RECURSION_DESIRED: u16 = 1 << 8;
const TYPE_A: u16 = 1;
const CLASS_IN: u16 = 1;

/// Stable rejection for a DNS datagram that cannot be answered canonically.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptiveDnsError {
    /// The received length is outside the supplied packet storage or UDP limit.
    Length,
    /// The packet is a response rather than a query.
    NotQuery,
    /// Only the ordinary DNS query opcode is supported.
    Opcode,
    /// Truncated requests are not sufficient authority for a reply.
    Truncated,
    /// Exactly one question is required.
    QuestionCount,
    /// The question name is truncated, compressed, or otherwise malformed.
    Name,
    /// The complete question type and class are absent.
    Question,
    /// The caller's packet storage cannot hold the bounded answer.
    Capacity,
}

/// Rewrites one DNS query in place as a bounded recovery-AP response.
///
/// Ordinary Internet-class IPv4 questions receive exactly one zero-TTL answer
/// containing `address`. Other valid question types receive a successful empty
/// response. Additional records supplied by the requester are deliberately
/// omitted. The returned value is the exact response length to send.
pub fn build_captive_dns_reply(
    packet: &mut [u8],
    received: usize,
    address: [u8; 4],
) -> Result<usize, CaptiveDnsError> {
    if received < DNS_HEADER_BYTES || received > packet.len() || received > CAPTIVE_DNS_PACKET_BYTES
    {
        return Err(CaptiveDnsError::Length);
    }

    let flags = read_u16(packet, 2);
    if flags & FLAGS_QUERY_RESPONSE != 0 {
        return Err(CaptiveDnsError::NotQuery);
    }
    if flags & FLAGS_OPCODE_MASK != 0 {
        return Err(CaptiveDnsError::Opcode);
    }
    if flags & FLAGS_TRUNCATED != 0 {
        return Err(CaptiveDnsError::Truncated);
    }
    if read_u16(packet, 4) != 1 {
        return Err(CaptiveDnsError::QuestionCount);
    }

    let question_end = question_end(&packet[..received])?;
    let question_type = read_u16(packet, question_end - 4);
    let question_class = read_u16(packet, question_end - 2);
    let answer_ipv4 = question_type == TYPE_A && question_class == CLASS_IN;
    let response_len = if answer_ipv4 {
        question_end
            .checked_add(IPV4_ANSWER_BYTES)
            .ok_or(CaptiveDnsError::Capacity)?
    } else {
        question_end
    };
    if response_len > packet.len() || response_len > CAPTIVE_DNS_PACKET_BYTES {
        return Err(CaptiveDnsError::Capacity);
    }

    let response_flags =
        FLAGS_QUERY_RESPONSE | FLAGS_AUTHORITATIVE | (flags & FLAGS_RECURSION_DESIRED);
    write_u16(packet, 2, response_flags);
    write_u16(packet, 4, 1);
    write_u16(packet, 6, u16::from(answer_ipv4));
    write_u16(packet, 8, 0);
    write_u16(packet, 10, 0);

    if answer_ipv4 {
        let answer = &mut packet[question_end..response_len];
        // A compression pointer to the sole question name at byte 12.
        answer[0..2].copy_from_slice(&[0xc0, 0x0c]);
        answer[2..4].copy_from_slice(&TYPE_A.to_be_bytes());
        answer[4..6].copy_from_slice(&CLASS_IN.to_be_bytes());
        answer[6..10].copy_from_slice(&0_u32.to_be_bytes());
        answer[10..12].copy_from_slice(&4_u16.to_be_bytes());
        answer[12..16].copy_from_slice(&address);
    }

    Ok(response_len)
}

fn question_end(packet: &[u8]) -> Result<usize, CaptiveDnsError> {
    let mut cursor = DNS_HEADER_BYTES;
    let name_start = cursor;
    loop {
        let Some(&length) = packet.get(cursor) else {
            return Err(CaptiveDnsError::Name);
        };
        cursor += 1;
        if length == 0 {
            break;
        }
        // Compression pointers in a request are unnecessary for the one-name
        // recovery protocol and would make exact bounded validation recursive.
        if length & 0xc0 != 0 || length > 63 {
            return Err(CaptiveDnsError::Name);
        }
        cursor = cursor
            .checked_add(usize::from(length))
            .ok_or(CaptiveDnsError::Name)?;
        if cursor > packet.len() || cursor - name_start > 255 {
            return Err(CaptiveDnsError::Name);
        }
    }
    if cursor - name_start > 255 {
        return Err(CaptiveDnsError::Name);
    }
    cursor
        .checked_add(4)
        .filter(|end| *end <= packet.len())
        .ok_or(CaptiveDnsError::Question)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(question_type: u16) -> ([u8; CAPTIVE_DNS_PACKET_BYTES], usize) {
        let mut packet = [0_u8; CAPTIVE_DNS_PACKET_BYTES];
        packet[0..2].copy_from_slice(&0x4a21_u16.to_be_bytes());
        packet[2..4].copy_from_slice(&FLAGS_RECURSION_DESIRED.to_be_bytes());
        packet[4..6].copy_from_slice(&1_u16.to_be_bytes());
        let question = [
            7, b'a', b'l', b'u', b'm', b'i', b'n', b'a', 5, b'l', b'o', b'c', b'a', b'l', 0,
        ];
        let mut end = DNS_HEADER_BYTES;
        packet[end..end + question.len()].copy_from_slice(&question);
        end += question.len();
        packet[end..end + 2].copy_from_slice(&question_type.to_be_bytes());
        packet[end + 2..end + 4].copy_from_slice(&CLASS_IN.to_be_bytes());
        (packet, end + 4)
    }

    #[test]
    fn ipv4_query_gets_one_zero_ttl_authoritative_answer() {
        let (mut packet, received) = query(TYPE_A);
        let reply = build_captive_dns_reply(&mut packet, received, [192, 168, 4, 1]).unwrap();

        assert_eq!(reply, received + IPV4_ANSWER_BYTES);
        assert_eq!(read_u16(&packet, 0), 0x4a21);
        assert_eq!(read_u16(&packet, 2), 0x8500);
        assert_eq!(read_u16(&packet, 4), 1);
        assert_eq!(read_u16(&packet, 6), 1);
        assert_eq!(read_u16(&packet, 8), 0);
        assert_eq!(read_u16(&packet, 10), 0);
        assert_eq!(&packet[received..received + 2], &[0xc0, 0x0c]);
        assert_eq!(read_u16(&packet, received + 2), TYPE_A);
        assert_eq!(read_u16(&packet, received + 4), CLASS_IN);
        assert_eq!(&packet[received + 6..received + 10], &[0, 0, 0, 0]);
        assert_eq!(read_u16(&packet, received + 10), 4);
        assert_eq!(&packet[received + 12..reply], &[192, 168, 4, 1]);
    }

    #[test]
    fn unsupported_type_gets_canonical_empty_success() {
        let (mut packet, received) = query(28);
        // An EDNS-shaped suffix must not be reflected or retained in counts.
        packet[10..12].copy_from_slice(&1_u16.to_be_bytes());
        packet[received..received + 11].copy_from_slice(&[0, 0, 41, 2, 0, 0, 0, 0, 0, 0, 0]);
        let reply = build_captive_dns_reply(&mut packet, received + 11, [192, 168, 4, 1]).unwrap();

        assert_eq!(reply, received);
        assert_eq!(read_u16(&packet, 2), 0x8500);
        assert_eq!(read_u16(&packet, 6), 0);
        assert_eq!(read_u16(&packet, 10), 0);
    }

    #[test]
    fn malformed_or_amplifying_queries_are_rejected() {
        let (mut packet, received) = query(TYPE_A);
        packet[4..6].copy_from_slice(&2_u16.to_be_bytes());
        assert_eq!(
            build_captive_dns_reply(&mut packet, received, [192, 168, 4, 1]),
            Err(CaptiveDnsError::QuestionCount)
        );

        let (mut packet, received) = query(TYPE_A);
        packet[12] = 0xc0;
        assert_eq!(
            build_captive_dns_reply(&mut packet, received, [192, 168, 4, 1]),
            Err(CaptiveDnsError::Name)
        );

        let (mut packet, received) = query(TYPE_A);
        packet[2..4].copy_from_slice(&FLAGS_TRUNCATED.to_be_bytes());
        assert_eq!(
            build_captive_dns_reply(&mut packet, received, [192, 168, 4, 1]),
            Err(CaptiveDnsError::Truncated)
        );
    }

    #[test]
    fn exact_storage_limit_is_enforced_before_mutation() {
        let (packet, received) = query(TYPE_A);
        let mut exact = packet[..received].to_vec();
        let before = exact.clone();
        assert_eq!(
            build_captive_dns_reply(&mut exact, received, [192, 168, 4, 1]),
            Err(CaptiveDnsError::Capacity)
        );
        assert_eq!(exact, before);
    }
}
