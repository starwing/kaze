#ifndef _kaze_h_
#define _kaze_h_

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#if defined(KZ_IMPLEMENTATION)
#define KZ_API __declspec(dllexport)
#else
#define KZ_API __declspec(dllimport)
#endif
#else
#define KZ_API
#endif

#define KZ_OK      (0)
#define KZ_FAIL    (-1) /* operation failed */
#define KZ_TOOBIG  (-2) /* enqueue data is too big */
#define KZ_BUSY    (-3) /* no data available or no enough space */
#define KZ_TIMEOUT (-4) /* operation timed out */

typedef struct kz_State        kz_State;
typedef struct kz_ReceivedData kz_ReceivedData;

KZ_API int kz_cleanup_host(const char *name);
KZ_API int kz_unlink(const char *name);

KZ_API kz_State *kz_new(const char *name, uint32_t ident, size_t netsize,
                        size_t hostsize);
KZ_API kz_State *kz_open(const char *name);
KZ_API void      kz_delete(kz_State *S);

KZ_API const char *kz_name(const kz_State *S);

KZ_API int kz_is_sidecar(const kz_State *S);
KZ_API int kz_is_host(const kz_State *S);

KZ_API int kz_try_push(kz_State *S, void *data, size_t size);
KZ_API int kz_push(kz_State *S, void *data, size_t size);
KZ_API int kz_push_until(kz_State *S, void *data, size_t size, int millis);

KZ_API int kz_try_pop(kz_State *S, kz_ReceivedData *data);
KZ_API int kz_pop(kz_State *S, kz_ReceivedData *data);
KZ_API int kz_pop_until(kz_State *S, kz_ReceivedData *data, int millis);

#define kz_data_len(x) ((x)->size)

KZ_API size_t      kz_data_count(const kz_ReceivedData *data);
KZ_API const char *kz_data_part(const kz_ReceivedData *data, size_t idx,
                                size_t *plen);
KZ_API void        kz_data_free(kz_ReceivedData *data);

typedef struct kz_ReceivedData {
    void  *refer;
    size_t head;
    size_t size;
} kz_ReceivedData;

#endif /* _kaze_h_ */

#if defined(KZ_IMPLEMENTATION) && !defined(kz_implemented)
#define kz_implemented

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

#define KZ_ALIGN sizeof(uint32_t)

// this must match the layout of the struct in the shared memory
typedef struct kz_RBHdr {
    uint32_t size;
    uint32_t head;
    uint32_t tail;
    uint32_t used;
    uint32_t need;
} kz_RBHdr;

typedef struct kz_RingBuffer {
    kz_RBHdr *hdr;
} kz_RingBuffer;

typedef struct kz_StateHdr {
    uint32_t size;          /* Size of the shared memory. 4GB max. */
    uint32_t sidecar_ident; /* Sidecar process identifier. */
    uint32_t sidecar_pid;   /* Sidecar process id. */
    uint32_t host_pid;      /* Host process id. */
    uint32_t netside_size;  /* Size of the net side buffer. */
    uint32_t hostside_size; /* Size of the host side buffer. */
} kz_StateHdr;

struct kz_State {
    int           shm_fd;
    uint32_t      self_pid;
    kz_StateHdr  *hdr;
    size_t        shmsize;
    kz_RingBuffer netside;
    kz_RingBuffer hostside;
    size_t        name_len;
    char          name[1];
};

/* utils */

static int kz_is_aligned_to(size_t size, size_t align) {
    assert((align & (align - 1)) == 0);
    return (size & (align - 1)) == 0;
}

static size_t kz_get_aligned_size(size_t size, size_t align) {
    assert((align & (align - 1)) == 0);
    return (size + align - 1) & ~(align - 1);
}

static uint32_t kz_read_u32le(const char *data) {
    uint32_t n;
#ifdef __BIG_ENDIAN__
    memcpy(&n, data, sizeof(n));
    n = __builtin_bswap32(n);
#else
    memcpy(&n, data, sizeof(n));
#endif
    return n;
}

static void kz_write_u32le(char *data, uint32_t n) {
#ifdef __BIG_ENDIAN__
    n = __builtin_bswap32(n);
#endif
    memcpy(data, &n, sizeof(n));
}

/* futex operations */

#if defined(__APPLE__)
// see <bsd/sys/ulock.h>, this is not public API
#define UL_COMPARE_AND_WAIT_SHARED 3
#define ULF_WAKE_ALL               0x00000100

__attribute__((weak_import)) extern int __ulock_wait(
    uint32_t operation, void *addr, uint64_t value,
    uint32_t timeout);  // timeout is microseconds
__attribute__((weak_import)) extern int __ulock_wake(
    uint32_t operation, void *addr, uint64_t wake_value);

#define USE_OS_SYNC_WAIT_ON_ADDRESS 1
// #    include <os/os_sync_wait_on_address.h>, this is public API but only
// since macOS 14.4
#define OS_CLOCK_MACH_ABSOLUTE_TIME    32
#define OS_SYNC_WAIT_ON_ADDRESS_SHARED 1
#define OS_SYNC_WAKE_BY_ADDRESS_SHARED 1
__attribute__((weak_import)) extern int os_sync_wait_on_address(
    void *addr, uint64_t value, size_t size, uint32_t flags);
__attribute__((weak_import)) extern int os_sync_wait_on_address_with_timeout(
    void *addr, uint64_t value, size_t size, uint32_t flags, uint32_t clockid,
    uint64_t timeout_ns);
__attribute__((weak_import)) extern int os_sync_wake_by_address_any(
    void *addr, size_t size, uint32_t flags);
__attribute__((weak_import)) extern int os_sync_wake_by_address_all(
    void *addr, size_t size, uint32_t flags);

#elif defined(__linux__)
#include <linux/futex.h> /* Definition of FUTEX_* constants */
#include <sys/syscall.h> /* Definition of SYS_* constants */
#include <unistd.h>
#elif defined(_WIN32)

static BOOL    g_Inited;
static VOID    WINAPI (*f_WakeByAddressSingle)(PVOID Address);
static VOID    WINAPI (*f_WakeByAddressAll)(PVOID Address);
static WINBOOL WINAPI (*f_WaitOnAddress)(
    volatile VOID *Address, PVOID CompareAddress, SIZE_T AddressSize,
    DWORD dwMilliseconds);

#endif

void kz_futex_init(void) {
#if defined(_WIN32)
    HANDLE lib;
    if (g_Inited) return;
    lib = LoadLibrary("KernelBase.dll");  // Windows 10
    if (lib) {
        f_WakeByAddressSingle = (VOID WINAPI(*)(PVOID Address))GetProcAddress(
            lib, "WakeByAddressSingle");
        f_WakeByAddressAll = (VOID WINAPI(*)(PVOID Address))GetProcAddress(
            lib, "WakeByAddressAll");
        f_WaitOnAddress = (WINBOOL WINAPI(*)(
            volatile VOID * Address, PVOID CompareAddress, SIZE_T AddressSize,
            DWORD dwMilliseconds)) GetProcAddress(lib, "WaitOnAddress");
    } else {
        f_WakeByAddressSingle = NULL;
        f_WakeByAddressAll = NULL;
        f_WaitOnAddress = NULL;
    }
    g_Inited = 1;
#endif
}

int kz_futex_wait(void *addr, uint64_t ifValue, int timeoutMillis) {
#if defined(__APPLE__)
    int ret;
    if (os_sync_wait_on_address_with_timeout && USE_OS_SYNC_WAIT_ON_ADDRESS) {
        if (timeoutMillis == 0) {
            ret = os_sync_wait_on_address((void *)addr, (uint64_t)ifValue, 4,
                                          OS_SYNC_WAIT_ON_ADDRESS_SHARED);
        } else {
            ret = os_sync_wait_on_address_with_timeout(
                (void *)addr, (uint64_t)ifValue, 4,
                OS_SYNC_WAIT_ON_ADDRESS_SHARED, OS_CLOCK_MACH_ABSOLUTE_TIME,
                timeoutMillis * 1000 * 1000);
        }
    } else if (__ulock_wait) {
        ret = __ulock_wait(UL_COMPARE_AND_WAIT_SHARED, (void *)addr,
                           (uint64_t)ifValue, timeoutMillis * 1000);
    } else {
        errno = ENOTSUP;
        return -1;
    }

    if (ret >= 0) {
        return 0;
    } else if (ret == -ETIMEDOUT || errno == ETIMEDOUT) {
        // timeout
        errno = ETIMEDOUT;
        return -1;
    } else if (errno == EAGAIN) {  // not observed on macOS; just in case
        return 0;                  // ifValue did not match
    }
    return -1;

#elif defined(__linux__)
    if (timeoutMillis == 0) {
        // specifying NULL would prevent the call from being interruptable
        // cf. https://outerproduct.net/futex-dictionary.html#linux
        timeoutMillis = INT_MAX;  // a long time
    }

    struct timespec ts = {.tv_sec = timeoutMillis / 1000,
                          .tv_nsec = (timeoutMillis % 1000) * 1000000};
    long ret = syscall(SYS_futex, (void *)addr, FUTEX_WAIT, ifValue, &ts, NULL,
                       0);

    if (ret == 0) {
        return 0;
    } else if (ret > 0 || errno == ETIMEDOUT) {
        return -1;
    } else if (errno == EAGAIN) {
        return 0;  // ifValue did not match
    }

    if (errno == ENOSYS) {
        errno = ENOTSUP;
    }
    return -1;

#else
#if defined(_WIN32)
    if (f_WaitOnAddress) {
        if (f_WaitOnAddress((void *)addr, &ifValue, 4, timeoutMillis)) {
            return 0;
        } else if (io_errno == 1460) {  // ERROR_TIMEOUT
            errno = io_errno;
            return -1;
        } else {
            errno = io_errno;
            return -1;
        }
    }
#endif
    errno = ENOTSUP;
    return -1;
#endif
}

int kz_futex_wake(void *addr, int wakeAll) {
#if defined(__APPLE__)
    int ret;
    if (wakeAll) {
        if (os_sync_wake_by_address_all && USE_OS_SYNC_WAIT_ON_ADDRESS) {
            ret = os_sync_wake_by_address_all(addr, 4,
                                              OS_SYNC_WAKE_BY_ADDRESS_SHARED);
        } else if (__ulock_wake) {
            ret = __ulock_wake(UL_COMPARE_AND_WAIT_SHARED | ULF_WAKE_ALL, addr,
                               0);
        } else {
            errno = ENOTSUP;
            return -1;
        }
    } else {
        if (os_sync_wake_by_address_any && USE_OS_SYNC_WAIT_ON_ADDRESS) {
            ret = os_sync_wake_by_address_any((void *)addr, 4,
                                              OS_SYNC_WAKE_BY_ADDRESS_SHARED);
        } else if (__ulock_wake) {
            ret = __ulock_wake(UL_COMPARE_AND_WAIT_SHARED, (void *)addr, 0);
        } else {
            errno = ENOTSUP;
            return -1;
        }
    }

    if (ret >= 0) {
        return 0;
    } else if (ret == -ENOENT || errno == ENOENT) {
        // none to wake up
        errno = ENOENT;
        return -1;
    }

    return -1;
#elif defined(__linux__)
    long ret = syscall(SYS_futex, (void *)addr, FUTEX_WAKE,
                       (wakeAll ? INT_MAX : 1), NULL, NULL, 0);
    if (ret == 0) {
        return -1;
    } else if (ret > 0) {
        return 0;
    }

    if (errno == ENOSYS) {
        errno = ENOTSUP;
    }
    return -1;
#else
#if defined(_WIN32)
    if (wakeAll && f_WakeByAddressAll) {
        f_WakeByAddressAll((void *)addr);
        return -1;
    } else if (f_WakeByAddressSingle) {
        f_WakeByAddressSingle((void *)addr);
        return -1;
    }
#endif

    CK_ARGUMENT_POTENTIALLY_UNUSED(env);
    CK_ARGUMENT_POTENTIALLY_UNUSED(addr);
    CK_ARGUMENT_POTENTIALLY_UNUSED(wakeAll);
    errno = ENOTSUP;
    return -1;
#endif
}

/* ring buffer */

#define rb_data(rb) (char *)((rb)->hdr + 1)

static size_t rb_requested_size(size_t size) {
    assert(kz_is_aligned_to(size, sizeof(uint32_t)));
    return kz_get_aligned_size(size + sizeof(kz_RBHdr), KZ_ALIGN);
}

static void rb_init(kz_RingBuffer *rb, kz_RBHdr *hdr, size_t size) {
    assert(kz_is_aligned_to(size, KZ_ALIGN));
    rb->hdr = hdr;
    hdr->size = size;
    hdr->head = 0;
    hdr->tail = 0;
    hdr->used = 0;
    hdr->need = 0;
}

static size_t rb_used(kz_RingBuffer *rb) {
    kz_RBHdr *hdr = rb->hdr;
    return __atomic_load_n(&hdr->used, __ATOMIC_ACQUIRE);
}

static size_t rb_space_size(kz_RingBuffer *rb) {
    kz_RBHdr *hdr = rb->hdr;
    return hdr->size - rb_used(rb);
}

static const char *shm;

static int rb_try_push(kz_RingBuffer *rb, const void *data, size_t size) {
    kz_RBHdr *hdr = rb->hdr;
    size_t    need_size, free_size, old_used;
    char     *start, *end;

    /* check if there is enough space */
    need_size = kz_get_aligned_size(size + sizeof(uint32_t), KZ_ALIGN);
    if (need_size > rb->hdr->size) {
        return KZ_TOOBIG;
    }

    free_size = rb_space_size(rb);
    if (free_size < need_size) {
        uint32_t addition_needed = (uint32_t)(need_size - free_size);
        __atomic_store_n(&hdr->need, addition_needed, __ATOMIC_RELEASE);
        return KZ_BUSY;
    }

    /* do the actual enqueuing */
    assert(hdr->tail + sizeof(uint32_t) <= hdr->size);
    start = rb_data(rb) + hdr->tail;
    end = rb_data(rb) + hdr->size;
    kz_write_u32le(start, size);
    start += sizeof(uint32_t);
    if (start + size <= end) {
        memcpy(start, data, size);
    } else {
        size_t remain = end - start;
        memcpy(start, data, remain);
        memcpy(rb_data(rb), (char *)data + remain, size - remain);
    }

    /* update the tail and used */
    printf("update tail from %d to %lu\n", hdr->tail,
           (hdr->tail + need_size) % hdr->size);
    hdr->tail = (hdr->tail + need_size) % hdr->size;
    assert(kz_is_aligned_to(hdr->tail, KZ_ALIGN));
    old_used = __atomic_fetch_add(&hdr->used, need_size, __ATOMIC_RELEASE);
    if (old_used == 0) {
        printf("wake on %ld (%p %p)\n", (char *)&rb->hdr->used - (char *)shm,
               &rb->hdr->used, shm);
        kz_futex_wake(&hdr->used, 0);
    }
    return KZ_OK;
}

static int rb_try_pop(kz_RingBuffer *rb, kz_ReceivedData *data) {
    kz_RBHdr *hdr = rb->hdr;
    size_t    used_size;
    char     *start;

    /* check if there is enough data */
    used_size = rb_used(rb);
    if (used_size == 0) return KZ_BUSY;
    assert(used_size >= sizeof(uint32_t));

    /* read the size of the data */
    start = rb_data(rb) + hdr->head;
    assert(start + sizeof(uint32_t) <= rb_data(rb) + hdr->size);
    printf("pop start=%d\n", hdr->head);
    data->refer = hdr;
    data->size = kz_read_u32le(start);
    printf("pop size=%lX\n", data->size);
    data->head = hdr->head + sizeof(uint32_t);
    return KZ_OK;
}

/* received data */

static void rb_pop_commit(const kz_ReceivedData *data) {
    kz_RBHdr *hdr = (kz_RBHdr *)data->refer;
    size_t    commit_size = kz_get_aligned_size(sizeof(uint32_t) + data->size,
                                                KZ_ALIGN);
    printf("update head from %d to %lu\n", hdr->head,
           (hdr->head + commit_size) % hdr->size);
    hdr->head = (hdr->head + commit_size) % hdr->size;
    assert(kz_is_aligned_to(hdr->head, KZ_ALIGN));
    __atomic_fetch_sub(&hdr->used, commit_size, __ATOMIC_RELEASE);
}

KZ_API size_t kz_data_count(const kz_ReceivedData *data) {
    kz_RBHdr *hdr = (kz_RBHdr *)data->refer;
    if (hdr == NULL) return 0;
    return data->head + data->size < hdr->size ? 1 : 2;
}

KZ_API const char *kz_data_part(const kz_ReceivedData *data, size_t idx,
                                size_t *plen) {
    kz_RBHdr *hdr = (kz_RBHdr *)data->refer;

    size_t cnt = kz_data_count(data);
    char  *buf = (char *)(hdr + 1);
    if (idx >= cnt) return NULL;
    if (idx == 0) {
        *plen = cnt == 1 ? data->size : hdr->size - data->size;
        return buf + data->head;
    }
    *plen = data->size - (hdr->size - data->head);
    return buf;
}

KZ_API void kz_data_free(kz_ReceivedData *data) {
    kz_RBHdr *hdr = (kz_RBHdr *)data->refer;
    size_t    new_need, size = data->size;
    rb_pop_commit(data);
    memset(data, 0, sizeof(*data));

    /* if the ring buffer is empty, try to wake up the sender */
    size = kz_get_aligned_size(size + sizeof(uint32_t), KZ_ALIGN);
    new_need = __atomic_sub_fetch(&hdr->need, size, __ATOMIC_ACQ_REL);
    if ((int32_t)new_need <= 0) {
        kz_futex_wake(&hdr->need, 1);
    }
}

/* shm check */

KZ_API int kz_is_sidecar(const kz_State *S) {
    return S->hdr->sidecar_pid == S->self_pid;
}

KZ_API int kz_is_host(const kz_State *S) {
    return S->hdr->host_pid == S->self_pid;
}

/* shm push & pop */

static kz_RingBuffer *get_rb(kz_State *S, int for_push) {
    shm = (char *)S->hdr;
    if (kz_is_sidecar(S) == (for_push != 0))
        return &S->netside;
    else
        return &S->hostside;
}

KZ_API int kz_try_push(kz_State *S, void *data, size_t size) {
    kz_RingBuffer *rb = get_rb(S, 1);
    return rb_try_push(rb, data, size);
}

KZ_API int kz_push(kz_State *S, void *data, size_t size) {
    kz_RingBuffer *rb = get_rb(S, 1);
    size_t need_size = kz_get_aligned_size(size + sizeof(uint32_t), KZ_ALIGN);
    for (;;) {
        int ret = rb_try_push(rb, data, size);
        if (ret != KZ_BUSY) return ret;
        kz_futex_wait(&rb->hdr->need, need_size, 0);
    }
}

KZ_API int kz_push_until(kz_State *S, void *data, size_t size, int millis) {
    kz_RingBuffer *rb = get_rb(S, 1);

    int    ret;
    size_t need_size = kz_get_aligned_size(size + sizeof(uint32_t), KZ_ALIGN);

    if (need_size > rb->hdr->size) {
        return KZ_TOOBIG;
    }

    ret = rb_try_push(rb, data, size);
    if (ret != KZ_BUSY) return ret;

    if (kz_futex_wait(&rb->hdr->need, need_size, millis) == -1)
        return errno == ETIMEDOUT ? KZ_TIMEOUT : KZ_FAIL;

    return rb_try_push(rb, data, size);
}

KZ_API int kz_try_pop(kz_State *S, kz_ReceivedData *data) {
    kz_RingBuffer *rb = get_rb(S, 0);
    return rb_try_pop(rb, data);
}

KZ_API int kz_pop(kz_State *S, kz_ReceivedData *data) {
    kz_RingBuffer *rb = get_rb(S, 0);
    for (;;) {
        int ret = rb_try_pop(rb, data);
        if (ret != KZ_BUSY) return ret;
        printf("wait on %ld (%p %p)\n", ((char *)&rb->hdr->used - (char *)shm),
               &rb->hdr->used, shm);
        kz_futex_wait(&rb->hdr->used, 0, 0);
    }
}

KZ_API int kz_pop_until(kz_State *S, kz_ReceivedData *data, int millis) {
    kz_RingBuffer *rb = get_rb(S, 0);

    int ret = rb_try_pop(rb, data);
    if (ret != KZ_BUSY) return ret;
    kz_futex_wait(&rb->hdr->used, 0, millis);
    return rb_try_pop(rb, data);
}

/* init & cleanup */

KZ_API const char *kz_name(const kz_State *S) { return S->name; }

static void kz_init_fail(int shm_fd) {
    int err = errno;
    close(shm_fd);
    errno = err;
}

static int kz_init(kz_State *S, const char *filename, uint32_t ident,
                   size_t netsize, size_t hostsize) {
    char       *data;
    struct stat statbuf;

    /* create a new shared memory object */
    S->shm_fd = shm_open(filename, O_CREAT | O_EXCL | O_RDWR, 0666);
    if (S->shm_fd == -1) return KZ_FAIL;

    /* check if the file already exists */
    if (fstat(S->shm_fd, &statbuf) == -1)
        return kz_init_fail(S->shm_fd), KZ_FAIL;
    if (statbuf.st_size != 0) {
        close(S->shm_fd);
        errno = EEXIST;
        return KZ_FAIL;
    }

    /* calcuate the size of the shared memory object */
    S->shmsize = kz_get_aligned_size(sizeof(kz_StateHdr)
                                         + rb_requested_size(netsize)
                                         + rb_requested_size(hostsize),
                                     sizeof(uint32_t));

    /* set the size of the shared memory object */
    if (ftruncate(S->shm_fd, S->shmsize) == -1)
        return kz_init_fail(S->shm_fd), KZ_FAIL;

    /* macOS the size of the shared memory object, may not same as ftruncate */
    if (fstat(S->shm_fd, &statbuf) == -1)
        return kz_init_fail(S->shm_fd), KZ_FAIL;
    S->shmsize = statbuf.st_size;

    /* init the shared memory object */
    S->hdr = (kz_StateHdr *)mmap(NULL, S->shmsize, PROT_READ | PROT_WRITE,
                                 MAP_SHARED, S->shm_fd, 0);
    if (S->hdr == MAP_FAILED) return kz_init_fail(S->shm_fd), KZ_FAIL;
    S->hdr->size = S->shmsize;
    S->hdr->sidecar_pid = S->self_pid;
    S->hdr->host_pid = 0;
    S->hdr->sidecar_ident = ident;
    S->hdr->netside_size = netsize;
    S->hdr->hostside_size = hostsize;

    data = (char *)(S->hdr + 1);
    rb_init(&S->netside, (kz_RBHdr *)data, netsize);
    data += sizeof(kz_RBHdr) + netsize;
    rb_init(&S->hostside, (kz_RBHdr *)data, hostsize);
    return KZ_OK;
}

static int kz_open_raw(kz_State *S, const char *filename) {
    const char *data;
    struct stat statbuf;

    S->shm_fd = shm_open(filename, O_RDWR, 0666);
    if (S->shm_fd == -1) return KZ_FAIL;

    if (fstat(S->shm_fd, &statbuf) == -1)
        return kz_init_fail(S->shm_fd), KZ_FAIL;
    S->shmsize = statbuf.st_size;
    if (S->shmsize == 0) {
        close(S->shm_fd);
        errno = ENOENT;
        return KZ_FAIL;
    }
    S->hdr = (kz_StateHdr *)mmap(NULL, statbuf.st_size, PROT_READ | PROT_WRITE,
                                 MAP_SHARED, S->shm_fd, 0);
    if (S->hdr == MAP_FAILED) return kz_init_fail(S->shm_fd), KZ_FAIL;

    if (S->hdr->size != S->shmsize || S->hdr->sidecar_pid == 0) {
        munmap(S->hdr, S->shmsize);
        close(S->shm_fd);
        errno = EBADF;
        return KZ_FAIL;
    }

    if (S->hdr->host_pid != 0) {
        munmap(S->hdr, S->shmsize);
        close(S->shm_fd);
        errno = EBUSY;
        return KZ_FAIL;
    }

    S->hdr->host_pid = S->self_pid;
    data = (char *)(S->hdr + 1);
    rb_init(&S->netside, (kz_RBHdr *)data, S->hdr->netside_size);
    data += sizeof(kz_RBHdr) + S->hdr->netside_size;
    rb_init(&S->hostside, (kz_RBHdr *)data, S->hdr->hostside_size);
    return KZ_OK;
}

KZ_API void kz_delete(kz_State *S) {
    if (kz_is_sidecar(S)) {
        shm_unlink(S->name);
    }
    munmap(S->hdr, S->shmsize);
    close(S->shm_fd);
    free(S);
}

KZ_API int kz_unlink(const char *filename) {
    return shm_unlink(filename) == 0 ? KZ_OK : KZ_FAIL;
}

KZ_API int kz_cleanup_host(const char *filename) {
    struct stat  statbuf;
    kz_StateHdr *hdr = NULL;
    int          shm_id = shm_open(filename, O_RDWR, 0666);
    if (shm_id == -1) return KZ_FAIL;

    if (fstat(shm_id, &statbuf) == -1) {
        return kz_init_fail(shm_id), KZ_FAIL;
    }
    if (statbuf.st_size == 0 || (size_t)statbuf.st_size < sizeof(kz_StateHdr)) {
        close(shm_id);
        errno = EINVAL;
        return KZ_FAIL;
    }
    hdr = (kz_StateHdr *)mmap(NULL, statbuf.st_size, PROT_READ | PROT_WRITE,
                              MAP_SHARED, shm_id, 0);
    if (hdr->size != statbuf.st_size) {
        close(shm_id);
        errno = EINVAL;
        return KZ_FAIL;
    }
    hdr->host_pid = 0;
    munmap(hdr, statbuf.st_size);
    close(shm_id);
    return KZ_OK;
}

KZ_API kz_State *kz_new(const char *name, uint32_t ident, size_t netsize,
                        size_t hostsize) {
    size_t name_len = strlen(name);

    kz_State *S = (kz_State *)malloc(sizeof(kz_State) + name_len);
    if (S == NULL) return NULL;
    memcpy(S->name, name, name_len);
    S->name_len = name_len;
    S->self_pid = getpid();

    if (kz_init(S, name, ident, netsize, hostsize) != KZ_OK) {
        free(S);
        return NULL;
    }
    return S;
}

KZ_API kz_State *kz_open(const char *name) {
    kz_State *S = (kz_State *)malloc(sizeof(kz_State));
    if (S == NULL) return NULL;
    memcpy(S->name, name, strlen(name));
    S->name_len = strlen(name);
    S->self_pid = getpid();

    if (kz_open_raw(S, name) != KZ_OK) {
        free(S);
        return NULL;
    }
    return S;
}

#endif /* KZ_IMPLEMENTATION */