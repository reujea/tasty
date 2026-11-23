//! One injected listener-open failure, taken by the next endpoint start whose future is first
//! polled on the injecting thread, so tests running beside it never meet it.
use std::{cell::Cell, io, net::SocketAddr};
#[derive(Clone, Copy)]
pub(crate) enum Fault {
    /// The bound endpoint listener cannot report its local address.
    EndpointAddr(i32),
    /// A native trust probe listener cannot bind.
    ProbeBind(i32),
    /// A bound native trust probe listener cannot report its local address.
    ProbeAddr(i32),
}
thread_local! {
    static NEXT: Cell<Option<Fault>> = const { Cell::new(None) };
}
pub(crate) fn inject(fault: Fault) {
    NEXT.with(|next| next.set(Some(fault)));
}
/// The fault one endpoint start took, applied where that failure would happen.
#[derive(Clone, Copy, Default)]
pub(crate) struct Injected(Option<Fault>);
pub(crate) fn take() -> Injected {
    Injected(NEXT.with(Cell::take))
}
impl Injected {
    pub(crate) fn endpoint_addr(self, address: io::Result<SocketAddr>) -> io::Result<SocketAddr> {
        match self.0 {
            Some(Fault::EndpointAddr(code)) => Err(io::Error::from_raw_os_error(code)),
            _ => address,
        }
    }
    pub(crate) fn probe_bind<T>(self, bound: io::Result<T>) -> io::Result<T> {
        match self.0 {
            Some(Fault::ProbeBind(code)) => Err(io::Error::from_raw_os_error(code)),
            _ => bound,
        }
    }
    pub(crate) fn probe_addr(self, address: io::Result<SocketAddr>) -> io::Result<SocketAddr> {
        match self.0 {
            Some(Fault::ProbeAddr(code)) => Err(io::Error::from_raw_os_error(code)),
            _ => address,
        }
    }
}
