
use libc::{addrinfo, c_char, c_int};
use std::ffi::{CStr};
use std::ptr;
use std::sync::LazyLock;

type GetAddrInfoFn = unsafe extern "C" fn(
    node: *const c_char,
    service: *const c_char,
    hints: *const addrinfo,
    res: *mut *mut addrinfo,
) -> c_int;

static REAL_GETADDRINFO: LazyLock<GetAddrInfoFn> = LazyLock::new(load_original_getaddrinfo);

fn load_original_getaddrinfo() -> GetAddrInfoFn {
    unsafe {
        std::mem::transmute(libc::dlsym(libc::RTLD_NEXT, b"getaddrinfo\0".as_ptr() as *const i8))
    }
}

#[no_mangle]
pub unsafe extern "C" fn getaddrinfo(
    node: *const c_char,
    service: *const c_char,
    hints: *const addrinfo,
    res: *mut *mut addrinfo,
) -> c_int {
    if !node.is_null() {
        let c_str = CStr::from_ptr(node);
        if let Ok(domain) = c_str.to_str() {
            if domain == "apresolve.spotify.com" {
                return libc::EAI_NONAME;
            }
        }
    }

    REAL_GETADDRINFO(node, service, hints, res)
}
