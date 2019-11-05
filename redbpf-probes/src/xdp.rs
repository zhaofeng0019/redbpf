use core::mem;
use core::ops::{Index, Range};
use core::slice;
use cty::*;

pub use crate::bindings::*;
use crate::maps::{PerfMap as PerfMapBase, PerfMapFlags};
use redbpf_macros::internal_helpers as helpers;

#[repr(u32)]
pub enum XdpAction {
    Aborted = xdp_action_XDP_ABORTED,
    Drop = xdp_action_XDP_DROP,
    Pass = xdp_action_XDP_PASS,
    Tx = xdp_action_XDP_TX,
    Redirect = xdp_action_XDP_REDIRECT,
}

pub enum Transport {
    TCP(*const tcphdr),
    UDP(*const udphdr),
}

impl Transport {
    #[inline]
    pub fn source(&self) -> u16 {
        let source = match *self {
            Transport::TCP(hdr) => unsafe { (*hdr).source },
            Transport::UDP(hdr) => unsafe { (*hdr).source },
        };
        u16::from_be(source)
    }

    #[inline]
    pub fn dest(&self) -> u16 {
        let dest = match *self {
            Transport::TCP(hdr) => unsafe { (*hdr).dest },
            Transport::UDP(hdr) => unsafe { (*hdr).dest },
        };
        u16::from_be(dest)
    }
}

pub struct XdpContext {
    pub ctx: *mut xdp_md,
}

impl XdpContext {
    #[inline]
    pub fn inner(&self) -> *mut xdp_md {
        self.ctx
    }

    #[inline]
    pub fn len(&self) -> u32 {
        unsafe {
            let ctx = *self.ctx;
            ctx.data_end - ctx.data
        }
    }

    #[inline]
    pub fn eth(&self) -> Option<*const ethhdr> {
        let ctx = unsafe { *self.ctx };
        let eth = ctx.data as *const ethhdr;
        let end = ctx.data_end as *const c_void;
        unsafe {
            if eth.add(1) as *const c_void > end {
                return None;
            }
        }
        Some(eth)
    }

    #[inline]
    pub fn ip(&self) -> Option<*const iphdr> {
        let eth = self.eth()?;
        unsafe {
            if (*eth).h_proto != u16::from_be(ETH_P_IP as u16) {
                return None;
            }

            let ip = eth.add(1) as *const iphdr;
            if ip.add(1) as *const c_void > (*self.ctx).data_end as *const c_void {
                return None;
            }
            Some(ip)
        }
    }

    #[inline]
    pub fn transport(&self) -> Option<Transport> {
        let ip = self.ip()?;
        let base = unsafe { ip.add(1) as *const u8 };
        let (transport, size) = match unsafe { (*ip).protocol } as u32 {
            IPPROTO_TCP => (Transport::TCP(base.cast()), mem::size_of::<tcphdr>()),
            IPPROTO_UDP => (Transport::UDP(base.cast()), mem::size_of::<udphdr>()),
            _ => return None,
        };
        unsafe {
            if base.add(size) > (*self.ctx).data_end as *const u8 {
                return None;
            }
        }

        Some(transport)
    }
    #[inline]
    pub fn data(&self) -> Option<Data> {
        use Transport::*;
        unsafe {
            let base = match self.transport()? {
                TCP(hdr) => hdr.add(1) as *mut u8,
                UDP(hdr) => hdr.add(1) as *mut u8,
            };
            Some(Data {
                ctx: self.ctx,
                base,
            })
        }
    }
}

pub struct Data {
    pub ctx: *const xdp_md,
    pub base: *const u8,
}

impl Data {
    #[inline]
    pub fn offset(&self) -> usize {
        unsafe { (self.base as u32 - (*self.ctx).data) as usize }
    }

    #[inline]
    pub fn len(&self) -> usize {
        unsafe { ((*self.ctx).data_end - self.base as u32) as usize }
    }

    #[inline]
    pub fn slice(&self, len: usize) -> Option<&[u8]> {
        unsafe {
            if self.base.add(len) > (*self.ctx).data_end as *const u8 {
                return None;
            }
            Some(slice::from_raw_parts(self.base, len))
        }
    }
}

#[repr(transparent)]
pub struct PerfMap<T>(PerfMapBase<T>);

impl<T> PerfMap<T> {
    pub const fn new() -> Self {
        Self(PerfMapBase::new())
    }

    #[inline]
    #[helpers]
    pub fn insert(&mut self, ctx: &XdpContext, data: T, packet_size: u32) {
        self.0.insert_with_flags(ctx.inner(), data, PerfMapFlags::with_xdp_size(packet_size))
    }

    #[inline]
    #[helpers]
    pub fn insert_with_flags(&mut self, ctx: &XdpContext, data: T, flags: PerfMapFlags) {
        self.0.insert_with_flags(ctx.inner(), data, flags)
    }
}
