/*
 * esp_map.c — p_mmap/p_munmap for libgit2 on esp-idf (Spike 7, Path 2).
 *
 * Replaces src/util/unix/map.c, which needs <sys/mman.h> (absent on
 * picolibc/esp-idf). libgit2 uses p_mmap read-only, to view pack files and the
 * index. We emulate it by allocating a buffer and reading the range into it.
 * Allocations go through git__malloc, so with PSRAM in the heap they land in
 * the 8 MB external RAM rather than the ~340 KB internal DRAM.
 *
 * There is deliberately NO cache here. A window cache was built and removed
 * on 2026-07-12: across four instrumented real-repo bench runs it scored
 * exactly 0 hits, because libgit2's mwindow layer reuses its open windows and
 * only genuinely new (offset, len) ranges ever reach p_mmap — and the one
 * memory bug it "fixed" (7.4 MB resident starving zlib) was caused by the
 * cache itself holding buffers past p_munmap. Free-at-munmap keeps
 * GIT_OPT_SET_MWINDOW_MAPPED_LIMIT honest by construction: when libgit2
 * releases a window, the memory really is back in git__malloc's pool. Full
 * trail: docs/tradeoff-curves/sync-commit-staging.md (final-bench section).
 * The stats counters below are kept so a future workload that *does* repeat
 * ranges (push? reconcile?) can be spotted before anyone rebuilds a cache.
 *
 * Limitation: writable/shared mappings are not written back.
 */

#include "git2_util.h"
#include "map.h"

#include <unistd.h>
#include <string.h>
#include <errno.h>
#include <stdint.h>

int git__page_size(size_t *page_size)
{
	*page_size = 4096;
	return 0;
}

int git__mmap_alignment(size_t *alignment)
{
	*alignment = 4096;
	return 0;
}

/* Diagnostics, read from the bench via esp_map_stats(). The signature predates
 * the cache removal: `hits` is always 0, `misses` counts every mapping, and
 * `cached_kb` now reports the LIVE mapped bytes (the mwindow working set). */
static uint32_t g_maps;
static uint64_t g_read_bytes;
static size_t g_live_bytes;

void esp_map_stats(uint32_t *hits, uint32_t *misses, uint32_t *read_kb, uint32_t *cached_kb)
{
	if (hits) *hits = 0;
	if (misses) *misses = g_maps;
	if (read_kb) *read_kb = (uint32_t)(g_read_bytes / 1024);
	if (cached_kb) *cached_kb = (uint32_t)(g_live_bytes / 1024);
}

static int read_range(int fd, off64_t offset, size_t len, unsigned char *data)
{
	size_t got = 0;

	if (lseek(fd, offset, SEEK_SET) < 0) {
		git_error_set(GIT_ERROR_OS, "failed to seek for mmap emulation");
		return -1;
	}
	while (got < len) {
		ssize_t n = read(fd, data + got, len - got);
		if (n < 0) {
			if (errno == EINTR)
				continue;
			git_error_set(GIT_ERROR_OS, "failed to read for mmap emulation");
			return -1;
		}
		if (n == 0)
			break; /* short file: zero-fill the tail, like a real mapping */
		got += (size_t)n;
	}
	if (got < len)
		memset(data + got, 0, len - got);
	return 0;
}

int p_mmap(git_map *out, size_t len, int prot, int flags, int fd, off64_t offset)
{
	unsigned char *data;

	GIT_UNUSED(prot);
	GIT_UNUSED(flags);
	GIT_MMAP_VALIDATE(out, len, prot, flags);

	out->data = NULL;
	out->len = 0;

	data = git__malloc(len);
	GIT_ERROR_CHECK_ALLOC(data);
	if (read_range(fd, offset, len, data) < 0) {
		git__free(data);
		return -1;
	}
	g_maps++;
	g_read_bytes += len;
	g_live_bytes += len;

	out->data = data;
	out->len = len;
	return 0;
}

int p_munmap(git_map *map)
{
	GIT_ASSERT_ARG(map);

	git__free(map->data);
	if (g_live_bytes >= map->len)
		g_live_bytes -= map->len;
	else
		g_live_bytes = 0;
	map->data = NULL;
	map->len = 0;
	return 0;
}
