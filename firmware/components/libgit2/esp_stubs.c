/*
 * esp_stubs.c — POSIX identity/symlink stubs for libgit2 on esp-idf
 * (Spike 7, Path 2).
 *
 * picolibc *declares* these in <unistd.h>/<pwd.h> but does not implement them:
 * esp-idf has no users, groups, processes, or symlinks. libgit2 calls them
 * while resolving config paths, ownership checks, and temp names. We provide
 * definitions that model "a single root user, no user database, no symlinks",
 * which is the truthful shape of the device's flat filesystem.
 */

#include <sys/types.h>
#include <unistd.h>
#include <pwd.h>
#include <errno.h>
#include <sys/time.h>
#include <sys/stat.h> /* stat() + S_IWUSR for utimes()/p_open() below */
#include <stdio.h>    /* remove(), rename() for the p_rename replacement */
#include <fcntl.h>    /* open(), O_* flags for the p_open()/p_creat() shims */
#include <stdarg.h>   /* va_list for the variadic p_open() */

#ifndef O_BINARY
#define O_BINARY 0 /* no text/binary distinction on esp-idf */
#endif
#ifndef O_CLOEXEC
#define O_CLOEXEC 0 /* no exec() on esp-idf, so close-on-exec is a no-op */
#endif

/* One implicit root user/group. */
uid_t getuid(void)  { return 0; }
uid_t geteuid(void) { return 0; }
gid_t getgid(void)  { return 0; }
gid_t getegid(void) { return 0; }

/* No process hierarchy. */
pid_t getppid(void)      { return 0; }
pid_t getpgid(pid_t pid) { (void)pid; return 0; }
pid_t getsid(pid_t pid)  { (void)pid; return 0; }

/* No user database: report "no such user" so libgit2 falls back to $HOME. */
int getpwuid_r(uid_t uid, struct passwd *pwd, char *buf, size_t buflen,
               struct passwd **result)
{
	(void)uid;
	(void)pwd;
	(void)buf;
	(void)buflen;
	*result = NULL;
	return 0;
}

/* No symlinks on FAT/LittleFS: nothing is ever a symbolic link. */
ssize_t readlink(const char *path, char *buf, size_t bufsiz)
{
	(void)path;
	(void)buf;
	(void)bufsiz;
	errno = EINVAL; /* POSIX: EINVAL == "named file is not a symbolic link" */
	return -1;
}

/* No symlinks on FAT: creating one is unsupported. libgit2 calls this only in
 * its "does this filesystem support symlinks?" probe (fs_path.c) and while
 * copying trees (futils.c) — a clean failure is the honest answer, and libgit2
 * treats the target as a normal file. (git_push referenced it; git_smoke, which
 * only touched the ODB, did not — hence it surfaces now.) */
int symlink(const char *target, const char *linkpath)
{
	(void)target;
	(void)linkpath;
	errno = ENOSYS;
	return -1;
}

/* FATFS/VFS can't set file times — but utimes() MUST still fail for a path that
 * doesn't exist. libgit2's git_futils_touch() is p_utimes(), and that is how the
 * loose ODB's `freshen` probe answers "does this object already exist?":
 * git_odb_write() SKIPS the write entirely when freshen succeeds (odb.c:1629).
 * A blanket `return 0` made every freshen succeed, so libgit2 believed every
 * object was already on disk and silently dropped ALL loose-object writes —
 * blobs/trees/commits never persisted, and write_tree then failed with
 * "invalid object specified". Gate success on existence: present → 0 (actually
 * setting the time is a cosmetic no-op we skip), absent → -1/ENOENT so freshen
 * reports "not found" and the real write proceeds. */
int utimes(const char *path, const struct timeval times[2])
{
	struct stat st;
	(void)times;
	if (stat(path, &st) != 0)
		return -1; /* stat set errno (ENOENT for a missing object) */
	return 0;
}

/* lwip implements getaddrinfo() but not gai_strerror(); libgit2's socket stream
 * (streams/socket.c) uses it only to format a connect-error message, so a
 * constant string is sufficient. Deliberately no <netdb.h> include: the symbol
 * is undefined at link (no macro/decl shadows it), so a plain definition is
 * safe, and the ABI (pointer vs. implicit-int return) matches on xtensa. */
const char *gai_strerror(int ecode)
{
	(void)ecode;
	return "getaddrinfo failure";
}

/* POSIX rename() atomically replaces an existing target; FATFS f_rename does
 * NOT (it fails with EEXIST) and FAT has no hardlinks, so libgit2's own
 * p_rename (link-then-rename, in posix.c) can't overwrite config/refs/HEAD/index
 * during their lock→commit. Provide replace semantics: drop the target, then
 * rename. Not crash-atomic (a crash between the two loses `to`), but FAT offers
 * no atomic replace — acceptable for the working copy. posix.c's original is
 * compiled as libgit2_unused_p_rename (see the component CMakeLists), so this is
 * the p_rename every caller links against. */
int p_rename(const char *from, const char *to)
{
	(void)remove(to); /* ignore ENOENT when `to` doesn't exist yet */
	return rename(from, to) == 0 ? 0 : -1;
}

/* libgit2 creates loose objects and packfiles with mode 0444 (read-only) — the
 * git convention that objects are immutable. FATFS honours that as the AM_RDO
 * attribute and then refuses to f_unlink the file (EACCES), and esp-idf's FATFS
 * VFS chmod() can't clear AM_RDO — so a written object can NEVER be deleted,
 * which breaks re-clone recovery and (later) fetch/repack. We force owner-write
 * into every create mode so libgit2's files stay writable and therefore
 * deletable. Immutability is only a safety hint on an appliance where nothing
 * but libgit2 touches these files. posix.c's originals are compiled as
 * libgit2_unused_p_open/p_creat (see the component CMakeLists), so these are the
 * definitions every other TU links against. Mirrors posix.c's p_open/p_creat. */
int p_open(const char *path, int flags, ...)
{
	mode_t mode = 0;
	if (flags & O_CREAT) {
		va_list arg_list;
		va_start(arg_list, flags);
		mode = (mode_t)va_arg(arg_list, int);
		va_end(arg_list);
		mode |= S_IWUSR; /* never create read-only → FATFS won't set AM_RDO */
	}
	return open(path, flags | O_BINARY | O_CLOEXEC, mode);
}

int p_creat(const char *path, mode_t mode)
{
	return open(path, O_WRONLY | O_CREAT | O_TRUNC | O_BINARY | O_CLOEXEC,
	            mode | S_IWUSR);
}
