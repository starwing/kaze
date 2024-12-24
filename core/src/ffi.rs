use std::os::raw::c_void;

pub const KZ_OK: i32 = 0;
pub const KZ_FAIL: i32 = -1;
pub const KZ_TOOBIG: i32 = -2;
pub const KZ_BUSY: i32 = -3;
pub const KZ_TIMEOUT: i32 = -4;
pub const KZ_LASTERR: i32 = -5;

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct kz_State(*mut c_void);

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct kz_ReceivedData {
    data: *const c_void,
    head: usize,
    size: usize,
}

#[link(name = "kaze")]
extern "C" {
    pub fn kz_unlink(name: *const i8) -> i32;
    pub fn kz_cleanup_host(name: *const i8) -> i32;

    pub fn kz_new(
        name: *const i8,
        ident: u32,
        netsize: usize,
        hostsize: usize,
    ) -> *mut kz_State;

    pub fn kz_open(name: *const i8) -> *mut kz_State;

    pub fn kz_delete(S: *mut kz_State);

    pub fn kz_name(S: *const kz_State) -> *const i8;

    pub fn kz_is_sidecar(S: *const kz_State) -> i32;

    pub fn kz_is_host(S: *const kz_State) -> i32;

    pub fn kz_try_push(
        S: *mut kz_State,
        data: *const c_void,
        size: usize,
    ) -> i32;

    pub fn kz_push(S: *mut kz_State, data: *const c_void, size: usize) -> i32;

    pub fn kz_push_until(
        S: *mut kz_State,
        data: *const c_void,
        size: usize,
        millis: i32,
    ) -> i32;

    pub fn kz_try_pop(S: *mut kz_State, data: *mut kz_ReceivedData) -> i32;

    pub fn kz_pop(S: *mut kz_State, data: *mut kz_ReceivedData) -> i32;

    pub fn kz_pop_until(
        S: *mut kz_State,
        data: *mut kz_ReceivedData,
        millis: i32,
    ) -> i32;

    pub fn kz_data_count(data: *const kz_ReceivedData) -> usize;

    pub fn kz_data_part(
        data: *const kz_ReceivedData,
        idx: usize,
        plen: *mut usize,
    ) -> *const i8;

    pub fn kz_data_free(data: *mut kz_ReceivedData);
}

#[inline]
pub unsafe fn kz_data_len(data: *const kz_ReceivedData) -> usize {
    (*data).size
}
