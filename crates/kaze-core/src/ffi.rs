use std::{
    ffi::{c_char, c_int},
    os::raw::c_void,
};

pub const KZ_OK: c_int = 0;
pub const KZ_INVALID: c_int = -1;
pub const KZ_FAIL: c_int = -2;
pub const KZ_CLOSED: c_int = -3;
pub const KZ_TOOBIG: c_int = -4;
pub const KZ_AGAIN: c_int = -5;
pub const KZ_BUSY: c_int = -6;
pub const KZ_TIMEOUT: c_int = -7;

pub const KZ_CREATE: c_int = 1 << 16;
pub const KZ_EXCL: c_int = 1 << 17;
pub const KZ_RESET: c_int = 1 << 18;

pub const KZ_READ: c_int = 1 << 0;
pub const KZ_WRITE: c_int = 1 << 1;
pub const KZ_BOTH: c_int = KZ_READ | KZ_WRITE;

pub const KZ_MAX_SIZE: usize = 0xFFFFFFFFusize;

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct kz_State(*mut c_void);

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct kz_Context {
    state: *const c_void,
    pos: usize,
    len: usize,
    pub(crate) result: i32,
}

#[link(name = "kaze")]
unsafe extern "C" {
    pub fn kz_aligned(buf_size: usize, page_size: usize) -> usize;
    pub fn kz_exists(
        name: *const c_char,
        powner: *mut c_int,
        puser: *mut c_int,
    ) -> i32;
    pub fn kz_unlink(name: *const c_char) -> i32;

    // use sys::io::errno() instead.
    // pub fn kz_failerror() -> *const c_char;
    // pub fn kz_freefailerror(msg: *const c_char);

    pub fn kz_open(
        name: *const c_char,
        flags: c_int,
        bufsize: usize,
    ) -> *mut kz_State;
    pub fn kz_close(S: *mut kz_State);
    pub fn kz_shutdown(S: *mut kz_State, mode: c_int) -> c_int;

    pub fn kz_name(S: *const kz_State) -> *const c_char;
    pub fn kz_size(S: *const kz_State) -> usize;
    pub fn kz_pid(S: *const kz_State) -> c_int;
    pub fn kz_isowner(S: *const kz_State) -> c_int;
    pub fn kz_isclosed(S: *const kz_State) -> c_int;

    pub fn kz_read(S: *mut kz_State, ctx: *mut kz_Context) -> c_int;
    pub fn kz_write(
        S: *mut kz_State,
        ctx: *mut kz_Context,
        len: usize,
    ) -> c_int;

    pub fn kz_buffer(ctx: *mut kz_Context, plen: *mut usize) -> *mut c_char;
    pub fn kz_commit(ctx: *mut kz_Context, len: usize) -> c_int;
    pub fn kz_cancel(ctx: *mut kz_Context);
    pub fn kz_isread(ctx: *const kz_Context) -> c_int;

    pub fn kz_wait(S: *mut kz_State, len: usize, millis: c_int) -> c_int;
    pub fn kz_waitcontext(ctx: *mut kz_Context, millis: c_int) -> c_int;
}
