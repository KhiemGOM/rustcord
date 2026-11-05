//! Quality of Service High Packet Priority: asks Windows to mark the call's UDP packets as voice
//! traffic (DSCP 46), which a router can put ahead of everything else.

use std::os::windows::io::AsRawSocket;
use tokio::net::UdpSocket;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::QoS::{QOS_NON_ADAPTIVE_FLOW, QOS_VERSION, QOSAddSocketToFlow, QOSCloseHandle, QOSCreateHandle, QOSTrafficTypeVoice};
use windows::Win32::Networking::WinSock::SOCKET;

pub struct Qos(usize);

impl Drop for Qos {
    fn drop(&mut self) {
        unsafe {
            let _ = QOSCloseHandle(HANDLE(self.0 as *mut _));
        }
    }
}

/// The socket has to be connected already.
pub fn mark(udp: &UdpSocket) -> Option<Qos> {
    unsafe {
        let mut h = HANDLE::default();
        QOSCreateHandle(&QOS_VERSION { MajorVersion: 1, MinorVersion: 0 }, &mut h).ok()?;
        let q = Qos(h.0 as usize);
        let mut flow = 0u32;
        QOSAddSocketToFlow(h, SOCKET(udp.as_raw_socket() as usize), None, QOSTrafficTypeVoice, Some(QOS_NON_ADAPTIVE_FLOW), &mut flow).ok()?;
        Some(q)
    }
}
