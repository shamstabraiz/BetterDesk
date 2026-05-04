// Package crypto provides cryptographic utilities for the Yomie server.
// Includes AddrMangle (address encoding/decoding) and Ed25519 key management.
package crypto

import (
	"encoding/binary"
	"fmt"
	"net"
)

// AddrMangle encodes and decodes socket addresses in the format used by RustDesk protocol.
//
// Format:
//   IPv4: [4 bytes IP] + [2 bytes port big-endian] = 6 bytes
//   IPv6: [16 bytes IP] + [2 bytes port big-endian] = 18 bytes

// EncodeAddr encodes a UDP address into the mangled byte format.
func EncodeAddr(addr *net.UDPAddr) []byte {
	ip4 := addr.IP.To4()
	if ip4 != nil {
		// IPv4: 4 bytes IP + 2 bytes port
		buf := make([]byte, 6)
		copy(buf[0:4], ip4)
		binary.BigEndian.PutUint16(buf[4:6], uint16(addr.Port))
		return buf
	}
	// IPv6: 16 bytes IP + 2 bytes port
	ip6 := addr.IP.To16()
	buf := make([]byte, 18)
	copy(buf[0:16], ip6)
	binary.BigEndian.PutUint16(buf[16:18], uint16(addr.Port))
	return buf
}

// DecodeAddr decodes a mangled byte slice back to a UDP address.
// Returns error if the byte slice has an unexpected length.
func DecodeAddr(b []byte) (*net.UDPAddr, error) {
	switch len(b) {
	case 6:
		// IPv4
		ip := net.IP(make([]byte, 4))
		copy(ip, b[0:4])
		port := binary.BigEndian.Uint16(b[4:6])
		return &net.UDPAddr{IP: ip, Port: int(port)}, nil
	case 18:
		// IPv6
		ip := net.IP(make([]byte, 16))
		copy(ip, b[0:16])
		port := binary.BigEndian.Uint16(b[16:18])
		return &net.UDPAddr{IP: ip, Port: int(port)}, nil
	default:
		return nil, fmt.Errorf("addr_mangle: unexpected length %d (expected 6 or 18)", len(b))
	}
}

// EncodeAddrFromString encodes a host:port string into the mangled byte format.
func EncodeAddrFromString(address string) ([]byte, error) {
	addr, err := net.ResolveUDPAddr("udp", address)
	if err != nil {
		return nil, fmt.Errorf("addr_mangle: failed to resolve %q: %w", address, err)
	}
	return EncodeAddr(addr), nil
}

// DecodeAddrToString decodes mangled bytes and returns a "host:port" string.
func DecodeAddrToString(b []byte) (string, error) {
	addr, err := DecodeAddr(b)
	if err != nil {
		return "", err
	}
	return addr.String(), nil
}
