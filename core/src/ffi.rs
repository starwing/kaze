use std::{ffi::c_char, os::raw::c_void};

pub const KZ_OK: i32 = 0;
pub const KZ_FAIL: i32 = -1;
pub const KZ_CLOSED: i32 = -2;
pub const KZ_INVALID: i32 = -3;
pub const KZ_TOOBIG: i32 = -4;
pub const KZ_BUSY: i32 = -5;
pub const KZ_TIMEOUT: i32 = -6;
pub const KZ_LASTERR: i32 = -7;

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct kz_State(*mut c_void);

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct kz_PushContext {
    refer: *const c_void,
    head: usize,
    size: usize,
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct kz_PopContext {
    data: *const c_void,
    head: usize,
    size: usize,
}

#[link(name = "kaze")]
extern "C" {
    pub fn kz_unlink(name: *const c_char) -> i32;

    pub fn kz_new(
        name: *const c_char,
        ident: u32,
        bufsize: usize,
    ) -> *mut kz_State;

    pub fn kz_open(name: *const c_char) -> *mut kz_State;

    pub fn kz_delete(S: *mut kz_State);

    pub fn kz_name(S: *const kz_State) -> *const i8;
    pub fn kz_ident(S: *const kz_State) -> u32;
    pub fn kz_pid(S: *const kz_State) -> i32;
    pub fn kz_owner(S: *const kz_State, sender: *mut i32, receiver: *mut i32);
    pub fn kz_set_owner(S: *const kz_State, sender: i32, receiver: i32);
    pub fn kz_used(S: *const kz_State) -> usize;
    pub fn kz_size(S: *const kz_State) -> usize;

    pub fn kz_try_push(
        S: *mut kz_State,
        ctx: *mut kz_PushContext,
        len: usize,
    ) -> i32;

    pub fn kz_push(
        S: *mut kz_State,
        ctx: *mut kz_PushContext,
        len: usize,
    ) -> i32;

    pub fn kz_push_until(
        S: *mut kz_State,
        ctx: *mut kz_PushContext,
        len: usize,
        millis: i32,
    ) -> i32;

    pub fn kz_push_buffer(
        ctx: *mut kz_PushContext,
        part: i32,
        plen: *mut usize,
    ) -> *mut c_char;

    pub fn kz_push_commit(ctx: *mut kz_PushContext, len: usize) -> i32;

    pub fn kz_try_pop(S: *mut kz_State, ctx: *mut kz_PopContext) -> i32;

    pub fn kz_pop(S: *mut kz_State, ctx: *mut kz_PopContext) -> i32;

    pub fn kz_pop_until(
        S: *mut kz_State,
        ctx: *mut kz_PopContext,
        millis: i32,
    ) -> i32;

    pub fn kz_pop_buffer(
        ctx: *const kz_PopContext,
        part: i32,
        plen: *mut usize,
    ) -> *const c_char;

    pub fn kz_pop_commit(ctx: *mut kz_PopContext);
}
