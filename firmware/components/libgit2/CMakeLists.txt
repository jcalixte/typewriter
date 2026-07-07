# libgit2 as an ESP-IDF component (Spike 7, Path 2 — Gate A probe).
#
# Builds libgit2 1.9.4 (the exact version libgit2-sys 0.18.5 expects, so git2's
# safe Rust API can bind it in system mode later) from an on-disk source tree
# pointed to by $ENV{LIBGIT2_SRC}, configured for mbedTLS.
#
# Why a component and not CFLAGS injection (Path 1): registering as a component
# makes libgit2 inherit the lwip / mbedtls / pthread / vfs / newlib include and
# link graph. Path 1 died because manually injecting one component's includes
# cascades into the next (arpa/inet.h -> sys/ioctl.h -> ...). The component
# model resolves that graph for us.
#
# GATE A SCOPE: prove libgit2's C compiles under the esp-idf/xtensa toolchain.
# Not wired to any Rust caller yet — esp-idf builds all components (COMPONENTS
# is empty in esp-idf-sys's project template), so this compiles regardless.

set(LG2 $ENV{LIBGIT2_SRC})
if(NOT LG2 OR NOT EXISTS "${LG2}/include/git2.h")
    # No source provided → register an empty component and bail. This keeps the
    # editor build (`just build`, no env) working: esp-idf builds every
    # component, but with nothing to compile this one is a no-op. Only `git`
    # builds set LIBGIT2_SRC. (Once libgit2 is vendored into the repo this
    # branch becomes dead — LIBGIT2_SRC will always resolve.)
    message(STATUS "libgit2 component: LIBGIT2_SRC unset/invalid — empty component (git feature off)")
    idf_component_register()
    return()
endif()

# Source list mirrors libgit2-sys's build.rs, with two deltas:
#   - hashing/TLS = mbedtls (one hash file) instead of sha1dc + openssl
#   - bundled zlib compiled in (deps/zlib) instead of linking libz-sys
# The transports/ and streams/ dirs are compiled wholesale; each file guards
# itself on the feature macros in git2_features.h.
file(GLOB LG2_SRCS
    "${LG2}/src/libgit2/*.c"
    "${LG2}/src/libgit2/transports/*.c"
    "${LG2}/src/libgit2/streams/*.c"
    "${LG2}/src/util/*.c"
    "${LG2}/deps/llhttp/*.c"
    "${LG2}/deps/xdiff/*.c"
    "${LG2}/deps/pcre/*.c"
    "${LG2}/deps/zlib/*.c"
)
list(APPEND LG2_SRCS
    "${LG2}/src/util/allocators/failalloc.c"
    "${LG2}/src/util/allocators/stdalloc.c"
    "${LG2}/src/util/unix/realpath.c"
    "${LG2}/src/util/hash/mbedtls.c"   # SHA1 + SHA256 via mbedtls
    "${CMAKE_CURRENT_LIST_DIR}/esp_map.c"    # p_mmap via malloc+read (no <sys/mman.h>)
    "${CMAKE_CURRENT_LIST_DIR}/esp_stubs.c"  # getuid/readlink/utimes/... stubs
    # NOTE: unix/map.c replaced by esp_map.c — picolibc has no <sys/mman.h>.
    # NOTE: unix/process.c deliberately excluded — needs fork()/sys/wait.h,
    # only used by the SSH-exec transport we don't enable.
)

list(LENGTH LG2_SRCS LG2_NSRC)
if(LG2_NSRC LESS 50)
    message(FATAL_ERROR "libgit2 glob returned only ${LG2_NSRC} files — LIBGIT2_SRC likely wrong")
endif()
message(STATUS "libgit2 component: ${LG2_NSRC} source files from ${LG2}")

idf_component_register(
    SRCS ${LG2_SRCS}
    INCLUDE_DIRS "${LG2}/include"
    PRIV_INCLUDE_DIRS
        "${CMAKE_CURRENT_LIST_DIR}"        # our hand-written git2_features.h
        "${LG2}/src/libgit2"
        "${LG2}/src/util"
        "${LG2}/src/util/hash"
        "${LG2}/deps/llhttp"
        "${LG2}/deps/xdiff"
        "${LG2}/deps/pcre"
        "${LG2}/deps/zlib"
    REQUIRES mbedtls lwip pthread vfs newlib
)

# PCRE (bundled regex backend) compile-time config — copied from libgit2-sys's
# build.rs. Global -D because libgit2 shares one config.h across the tree.
target_compile_definitions(${COMPONENT_LIB} PRIVATE
    GIT_REGEX_BUILTIN=1
    HAVE_STDINT_H=1
    HAVE_MEMMOVE=1
    NO_RECURSE=1
    NEWLINE=10
    POSIX_MALLOC_THRESHOLD=10
    LINK_SIZE=2
    PARENS_NEST_LIMIT=250
    MATCH_LIMIT=10000000
    MATCH_LIMIT_RECURSION=MATCH_LIMIT
    MAX_NAME_SIZE=32
    MAX_NAME_COUNT=10000
)

# Force-include our platform shims into every libgit2 TU (lstat==stat, etc.).
target_compile_options(${COMPONENT_LIB} PRIVATE
    "-include" "${CMAKE_CURRENT_LIST_DIR}/esp_port.h"
)

# gcc 14 promoted these from warnings to hard errors; this vendored C (last
# tested on gcc <14) trips them benignly (int32_t/long pointer aliasing on a
# 32-bit target; internal iterator callback casts). Downgrade to warnings.
# Missing POSIX functions surface as implicit-declaration warnings, then as
# unresolved symbols at link — that's how we enumerate the shims still needed.
target_compile_options(${COMPONENT_LIB} PRIVATE
    -Wno-error=implicit-function-declaration
    -Wno-error=incompatible-pointer-types
    -Wno-error=implicit-int
)

# posix.c holds three low-level fs primitives we must replace for FATFS; compile
# the originals under throwaway names — scoped to this ONE file so no other TU is
# touched — and provide FATFS-correct versions in esp_stubs.c:
#   p_rename — FATFS f_rename can't replace an existing target and FAT has no
#     hardlinks, so libgit2's link-then-rename can't overwrite config/refs/HEAD/
#     index during its lock→commit. Ours does remove-then-rename.
#   p_open / p_creat — libgit2 creates objects/packs mode 0444; FATFS turns that
#     into AM_RDO and then won't f_unlink them (and chmod can't clear it), so
#     objects become undeletable (breaks re-clone/fetch/repack). Ours force
#     owner-write so nothing is ever created read-only.
set_source_files_properties(
    "${LG2}/src/util/posix.c"
    PROPERTIES COMPILE_DEFINITIONS
        "p_rename=libgit2_unused_p_rename;p_open=libgit2_unused_p_open;p_creat=libgit2_unused_p_creat"
)
