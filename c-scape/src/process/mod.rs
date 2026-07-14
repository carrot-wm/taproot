mod chdir;
mod chroot;
mod daemon;
mod egid;
mod euid;
mod exec;
mod getcwd;
mod gid;
mod groups;
mod kill;
mod pid;
mod pidfd;
mod priority;
mod rlimit;
mod sid;
mod system;
pub(crate) mod uid;
mod umask;
mod wait;

// this entire module is
// #[cfg(not(target_os = "wasi"))]
