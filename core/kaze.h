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
#define KZ_CLOSED  (-2) /* ring buffer is closed */
#define KZ_INVALID (-3) /* argument invalid */
#define KZ_TOOBIG  (-4) /* enqueue data is too big */
#define KZ_BUSY    (-5) /* no data available or no enough space */
#define KZ_TIMEOUT (-6) /* operation timed out */

typedef struct kz_State       kz_State;
typedef struct kz_PushContext kz_PushContext;
typedef struct kz_PopContext  kz_PopContext;

KZ_API int kz_unlink(const char *name);

KZ_API kz_State *kz_new(const char *name, uint32_t ident, size_t bufsize);
KZ_API kz_State *kz_open(const char *name);
KZ_API void      kz_delete(kz_State *S);

/* info */

KZ_API const char *kz_name(const kz_State *S);
KZ_API uint32_t    kz_ident(const kz_State *S);

KZ_API int  kz_pid(const kz_State *S);
KZ_API void kz_owner(const kz_State *S, int *sender, int *receiver);
KZ_API void kz_set_owner(kz_State *S, int sender, int receiver);

KZ_API size_t kz_used(const kz_State *S);
KZ_API size_t kz_size(const kz_State *S);

/* sync push */

KZ_API int kz_try_push(kz_State *S, kz_PushContext *ctx, size_t len);
KZ_API int kz_push(kz_State *S, kz_PushContext *ctx, size_t len);
KZ_API int kz_push_until(kz_State *S, kz_PushContext *ctx, size_t len,
                         int millis);

KZ_API char *kz_push_buffer(kz_PushContext *ctx, int part, size_t *plen);
KZ_API int   kz_push_commit(kz_PushContext *ctx, size_t len);

/* sync pop */

KZ_API int kz_try_pop(kz_State *S, kz_PopContext *ctx);
KZ_API int kz_pop(kz_State *S, kz_PopContext *ctx);
KZ_API int kz_pop_until(kz_State *S, kz_PopContext *ctx, int millis);

KZ_API const char *kz_pop_buffer(const kz_PopContext *ctx, int part,
                                 size_t *plen);
KZ_API void        kz_pop_commit(kz_PopContext *ctx);

struct kz_PushContext {
    void  *refer;
    size_t tail;
    size_t size;
};

struct kz_PopContext {
    void  *refer;
    size_t head;
    size_t size;
};

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

typedef struct kz_StateHdr {
    uint32_t size;         /* Size of the shared memory. 4GB max. */
    uint32_t used;         /* Used size of the ring buffer. */
    uint32_t ident;        /* Sidecar process identifier. */
    uint32_t sender_pid;   /* Sidecar process id. */
    uint32_t receiver_pid; /* Host process id. */
    uint32_t closed;       /* Closing flag. */
    uint32_t head;         /* Head of the ring buffer. */
    uint32_t tail;         /* Tail of the ring buffer. */
    uint32_t padding[8]; /* padding used and need to differentiate cacheline */
    uint32_t need;       /* Need size of the ring buffer. */
} kz_StateHdr;

struct kz_State {
    int          shm_fd;
    uint32_t     self_pid;
    kz_StateHdr *hdr;
    size_t       shmsize;
    size_t       name_len;
    char         name[1];
};

/* utils */

#define kz_data(hdr) (char *)(hdr + 1)

static int kz_is_aligned_to(size_t size, size_t align) {
    assert((align & (align - 1)) == 0);
    return (size & (align - 1)) == 0;
}

static size_t kz_get_aligned_size(size_t size, size_t align) {
    assert((align & (align - 1)) == 0);
    return (size + align - 1) & ~(align - 1);
}

static size_t kz_requested_size(size_t size) {
    assert(kz_is_aligned_to(size, sizeof(uint32_t)));
    return kz_get_aligned_size(size + sizeof(kz_StateHdr), KZ_ALIGN);
}

static size_t kz_space_size(kz_State *S) {
    kz_StateHdr *hdr = S->hdr;
    return hdr->size - kz_used(S);
}

static int kz_isclosed(kz_State *S) {
    kz_StateHdr *hdr = S->hdr;
    return __atomic_load_n(&hdr->closed, __ATOMIC_ACQUIRE) != 0;
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

/* push */

KZ_API int kz_try_push(kz_State *S, kz_PushContext *ctx, size_t size) {
    kz_StateHdr *hdr = S->hdr;
    size_t       need_size, free_size;

    /* check if there is enough space */
    need_size = kz_get_aligned_size(size + sizeof(uint32_t), KZ_ALIGN);
    if (need_size > hdr->size) return KZ_TOOBIG;

    free_size = kz_space_size(S);
    if (free_size < need_size) {
        uint32_t addition_needed = (uint32_t)(need_size - free_size);
        __atomic_store_n(&hdr->need, addition_needed, __ATOMIC_RELEASE);
        return KZ_BUSY;
    }

    ctx->refer = hdr;
    ctx->tail = hdr->tail;
    ctx->size = size;
    return KZ_OK;
}

KZ_API char *kz_push_buffer(kz_PushContext *ctx, int part, size_t *plen) {
    kz_StateHdr *hdr = (kz_StateHdr *)ctx->refer;

    size_t tail = ctx->tail + sizeof(uint32_t);
    size_t remain = hdr->size - tail;
    if (part == 0) {
        int tail_has_space = tail < hdr->size;
        *plen = (tail_has_space && tail + ctx->size > hdr->size) ? remain
                                                                 : ctx->size;
        return kz_data(hdr) + (tail_has_space ? tail : 0);
    } else if (part == 1 && ctx->size > remain) {
        *plen = ctx->size - remain;
        return kz_data(hdr);
    }
    *plen = 0;
    return NULL;
}

KZ_API int kz_push_commit(kz_PushContext *ctx, size_t len) {
    kz_StateHdr *hdr = (kz_StateHdr *)ctx->refer;
    size_t       old_used;
    if (len > ctx->size) return KZ_INVALID;
    kz_write_u32le(kz_data(hdr) + ctx->tail, len);
    len = kz_get_aligned_size(len + sizeof(uint32_t), KZ_ALIGN);
    hdr->tail = (hdr->tail + len) % hdr->size;
    assert(kz_is_aligned_to(hdr->tail, KZ_ALIGN));
    old_used = __atomic_fetch_add(&hdr->used, len, __ATOMIC_RELEASE);
    if (old_used == 0) kz_futex_wake(&hdr->used, 0);
    return KZ_OK;
}

KZ_API int kz_push(kz_State *S, kz_PushContext *ctx, size_t size) {
    size_t need_size = kz_get_aligned_size(size + sizeof(uint32_t), KZ_ALIGN);
    while (!kz_isclosed(S)) {
        int ret = kz_try_push(S, ctx, size);
        if (ret != KZ_BUSY) return ret;
        kz_futex_wait(&S->hdr->need, need_size, 0);
    }
    return KZ_CLOSED;
}

KZ_API int kz_push_until(kz_State *S, kz_PushContext *ctx, size_t size,
                         int millis) {
    size_t need_size = kz_get_aligned_size(size + sizeof(uint32_t), KZ_ALIGN);
    while (!kz_isclosed(S)) {
        int ret = kz_try_push(S, ctx, size);
        if (ret != KZ_BUSY) return ret;
        if (kz_futex_wait(&S->hdr->need, need_size, millis) == -1)
            return errno == ETIMEDOUT ? KZ_TIMEOUT : KZ_FAIL;
    }
    return KZ_CLOSED;
}

/* pop */

KZ_API int kz_try_pop(kz_State *S, kz_PopContext *ctx) {
    kz_StateHdr *hdr = S->hdr;
    size_t       used_size;
    char        *start;

    /* check if there is enough data */
    used_size = kz_used(S);
    if (used_size == 0) return KZ_BUSY;
    assert(used_size >= sizeof(uint32_t));

    /* read the size of the data */
    start = kz_data(hdr) + hdr->head;
    assert(start + sizeof(uint32_t) <= kz_data(hdr) + hdr->size);
    ctx->refer = hdr;
    ctx->size = kz_read_u32le(start);
    ctx->head = hdr->head + sizeof(uint32_t);
    return KZ_OK;
}

KZ_API const char *kz_pop_buffer(const kz_PopContext *ctx, int part,
                                 size_t *plen) {
    kz_StateHdr *hdr = (kz_StateHdr *)ctx->refer;

    size_t head = ctx->head;
    size_t remain = hdr->size - head;
    if (part == 0) {
        int head_has_data = head < hdr->size;
        *plen = (head_has_data && head + ctx->size > hdr->size) ? remain
                                                                : ctx->size;
        return kz_data(hdr) + (head_has_data ? head : 0);
    } else if (part == 1 && ctx->size > remain) {
        *plen = ctx->size - remain;
        return kz_data(hdr);
    }
    *plen = 0;
    return NULL;
}

KZ_API void kz_pop_commit(kz_PopContext *ctx) {
    kz_StateHdr *hdr = (kz_StateHdr *)ctx->refer;

    size_t commit_size = kz_get_aligned_size(sizeof(uint32_t) + ctx->size,
                                             KZ_ALIGN);
    size_t size, new_need;
    hdr->head = (hdr->head + commit_size) % hdr->size;
    assert(kz_is_aligned_to(hdr->head, KZ_ALIGN));
    __atomic_fetch_sub(&hdr->used, commit_size, __ATOMIC_RELEASE);

    /* if the ring buffer is empty, try to wake up the sender */
    size = kz_get_aligned_size(sizeof(uint32_t) + ctx->size, KZ_ALIGN);
    new_need = __atomic_sub_fetch(&hdr->need, size, __ATOMIC_ACQ_REL);
    if ((int32_t)new_need <= 0) kz_futex_wake(&hdr->need, 1);
}

KZ_API int kz_pop(kz_State *S, kz_PopContext *ctx) {
    while (!kz_isclosed(S)) {
        int ret = kz_try_pop(S, ctx);
        if (ret != KZ_BUSY) return ret;
        kz_futex_wait(&S->hdr->used, 0, 0);
    }
    return KZ_CLOSED;
}

KZ_API int kz_pop_until(kz_State *S, kz_PopContext *ctx, int millis) {
    while (!kz_isclosed(S)) {
        int ret = kz_try_pop(S, ctx);
        if (ret != KZ_BUSY) return ret;
        kz_futex_wait(&S->hdr->used, 0, millis);
    }
    return KZ_CLOSED;
}

/* info */

KZ_API const char *kz_name(const kz_State *S) { return S->name; }
KZ_API size_t      kz_size(const kz_State *S) { return S->hdr->size; }
KZ_API uint32_t    kz_ident(const kz_State *S) { return S->hdr->ident; }
KZ_API int         kz_pid(const kz_State *S) { return S->self_pid; }

KZ_API size_t kz_used(const kz_State *S) {
    kz_StateHdr *hdr = S->hdr;
    return __atomic_load_n(&hdr->used, __ATOMIC_ACQUIRE);
}

KZ_API void kz_owner(const kz_State *S, int *sender, int *receiver) {
    if (sender) *sender = S->hdr->sender_pid;
    if (receiver) *receiver = S->hdr->receiver_pid;
}

KZ_API void kz_set_owner(kz_State *S, int sender, int receiver) {
    if (sender >= 0) S->hdr->sender_pid = sender;
    if (receiver >= 0) S->hdr->receiver_pid = receiver;
}

/* init & cleanup */

static void kz_init_fail(int shm_fd) {
    int err = errno;
    close(shm_fd);
    errno = err;
}

static int kz_init(kz_State *S, const char *filename, uint32_t ident,
                   size_t bufsize) {
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
    S->shmsize = kz_get_aligned_size(
        sizeof(kz_StateHdr) + kz_requested_size(bufsize), KZ_ALIGN);

    /* set the size of the shared memory object */
    if (ftruncate(S->shm_fd, S->shmsize) == -1)
        return kz_init_fail(S->shm_fd), KZ_FAIL;

    /* macOS the size of the shared memory object, may not same as ftruncate
     */
    if (fstat(S->shm_fd, &statbuf) == -1)
        return kz_init_fail(S->shm_fd), KZ_FAIL;
    S->shmsize = statbuf.st_size;

    /* init the shared memory object */
    S->hdr = (kz_StateHdr *)mmap(NULL, S->shmsize, PROT_READ | PROT_WRITE,
                                 MAP_SHARED, S->shm_fd, 0);
    if (S->hdr == MAP_FAILED) return kz_init_fail(S->shm_fd), KZ_FAIL;
    memset(S->hdr, 0, sizeof(kz_StateHdr));
    S->hdr->size = S->shmsize - sizeof(kz_StateHdr);
    S->hdr->ident = ident;
    return KZ_OK;
}

static int kz_open_raw(kz_State *S, const char *filename) {
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

    if (S->shmsize != sizeof(kz_StateHdr) + S->hdr->size) {
        munmap(S->hdr, S->shmsize);
        close(S->shm_fd);
        errno = EBADF;
        return KZ_FAIL;
    }
    return KZ_OK;
}

KZ_API void kz_delete(kz_State *S) {
    __atomic_store_n(&S->hdr->closed, 1, __ATOMIC_RELAXED);
    kz_futex_wake(&S->hdr->used, 1);
    kz_futex_wake(&S->hdr->need, 1);
    munmap(S->hdr, S->shmsize);
    close(S->shm_fd);
    free(S);
}

KZ_API int kz_unlink(const char *filename) {
    return shm_unlink(filename) == 0 ? KZ_OK : KZ_FAIL;
}

KZ_API kz_State *kz_new(const char *name, uint32_t ident, size_t bufsize) {
    size_t name_len = strlen(name);

    kz_State *S = (kz_State *)malloc(sizeof(kz_State) + name_len);
    if (S == NULL) return NULL;
    memcpy(S->name, name, name_len);
    S->name_len = name_len;
    S->self_pid = getpid();

    if (kz_init(S, name, ident, bufsize) != KZ_OK) {
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