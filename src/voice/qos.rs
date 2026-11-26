//! Quality of Service High Packet Priority: asks Windows to mark the call's UDP packets as voice
//! traffic (DSCP 46), which a router can put ahead of everything else.

use std::os::windows::io::AsRawSocket;
use tokio::net::UdpSocket;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::QoS::{QOS_NON_ADAPTIVE_FLOW, QOS_VERSION, QOSAddSocketToFlow, QOSCloseHandle, QOSCreateHandle, QOSTrafficTypeVoice};
use windows::Win32::Networking::WinSock::{IPPROTO_IP, IPPROTO_UDP, SOCKET, SOL_SOCKET, SO_RCVBUF, SO_SNDBUF, getsockopt, setsockopt};

const UDP_SEND_MSG_SIZE: i32 = 2;
const IP_ECN: i32 = 50;

pub struct Qos(usize);

impl Drop for Qos {
    fn drop(&mut self) {
        unsafe {
            let _ = QOSCloseHandle(HANDLE(self.0 as *mut _));
        }
    }
}

/// Tunes UDP socket buffers and probes Windows USO / ECN support (best-effort, non-fatal).
pub fn tune_udp(udp: &UdpSocket) -> bool {
    let sock = SOCKET(udp.as_raw_socket() as usize);
    let buf: i32 = 256 * 1024;
    let ecn: i32 = 1;
    let mut uso: i32 = 0;
    let mut uso_len = std::mem::size_of::<i32>() as i32;
    unsafe {
        let _ = setsockopt(sock, SOL_SOCKET, SO_SNDBUF, Some(std::slice::from_raw_parts((&raw const buf).cast::<u8>(), 4)));
        let _ = setsockopt(sock, SOL_SOCKET, SO_RCVBUF, Some(std::slice::from_raw_parts((&raw const buf).cast::<u8>(), 4)));
        let _ = setsockopt(sock, IPPROTO_IP.0, IP_ECN, Some(std::slice::from_raw_parts((&raw const ecn).cast::<u8>(), 4)));
        getsockopt(sock, IPPROTO_UDP.0, UDP_SEND_MSG_SIZE, windows::core::PSTR((&raw mut uso).cast::<u8>()), &mut uso_len) == 0
    }
}

/// The socket has to be connected already.
pub fn mark(udp: &UdpSocket) -> Option<Qos> {
    unsafe {
        let mut h = HANDLE::default();
        QOSCreateHandle(&QOS_VERSION { MajorVersion: 1, MinorVersion: 0 }, &mut h).as_bool().then_some(())?;
        let q = Qos(h.0 as usize);
        let mut flow = 0u32;
        QOSAddSocketToFlow(h, SOCKET(udp.as_raw_socket() as usize), None, QOSTrafficTypeVoice, Some(QOS_NON_ADAPTIVE_FLOW), &mut flow).as_bool().then_some(())?;
        Some(q)
    }
}
