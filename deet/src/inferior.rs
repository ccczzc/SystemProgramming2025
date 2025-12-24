use crate::dwarf_data::DwarfData;
use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::process::Child;
use std::process::Command;

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, u64),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(signal::Signal),
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}
#[derive(Clone)]
struct Breakpoint {
    addr: u64,
    orig_byte: u8,
}

pub struct Inferior {
    child: Child,
    addr_to_breakpoints: HashMap<u64, Breakpoint>,
    pending_signal: Option<signal::Signal>,
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>, breakpoints: &Vec<u64>) -> Option<Inferior> {
        // TODO: implement me!
        let mut cmd = Command::new(target);
        cmd.args(args);
        unsafe {
            cmd.pre_exec(child_traceme);
        }
        let child = cmd.spawn().ok()?;

        let mut res = Inferior {
            child,
            addr_to_breakpoints: HashMap::new(),
            pending_signal: None,
        };
        for bp in breakpoints {
            res.set_breakpoint(*bp).ok()?;
        }
        match res.wait(Some(WaitPidFlag::WUNTRACED)).ok()? {
            Status::Stopped(signal, _rip) => {
                if signal != Signal::SIGTRAP {
                    eprintln!("WaitStatus::Stopped : Not signaled by SIGTRAP!");
                    return None;
                }
                // println!("Check signal SIGTRAP succeed at address {:#x}!", rip);
            }
            _other => {
                eprintln!("Other Status!");
                return None;
            }
        }

        Some(res)
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&mut self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        let status = match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        };
        match status {
            Status::Stopped(sig, _) => self.pending_signal = Some(sig),
            Status::Signaled(sig) => self.pending_signal = Some(sig),
            _ => self.pending_signal = None,
        }
        Ok(status)
    }

    pub fn cont(&mut self) -> Result<Status, nix::Error> {
        let regs = ptrace::getregs(self.pid())?;
        let instruction_ptr = regs.rip - 1;
        if let Some(breakpoint) = self.addr_to_breakpoints.get(&instruction_ptr) {
            // Restore original byte at breakpoint
            self.write_byte(instruction_ptr, breakpoint.orig_byte)?;
            let mut new_regs = regs;
            new_regs.rip = instruction_ptr;
            ptrace::setregs(self.pid(), new_regs)?;
            ptrace::step(self.pid(), None)?;
            let status = self.wait(None)?;
            if let Status::Stopped(signal, _) = status {
                assert!(signal == Signal::SIGTRAP);
            } else {
                return Ok(status);
            }
            self.set_breakpoint(instruction_ptr)?;
        }
        let sig = match self.pending_signal {
            Some(Signal::SIGTRAP) => None,
            x => x,
        };
        ptrace::cont(self.pid(), sig)?;
        self.wait(None)
    }

    // step forward by one instruction
    pub fn step(&mut self) -> Result<Status, nix::Error> {
        let regs = ptrace::getregs(self.pid())?;
        let instruction_ptr = regs.rip - 1;
        if let Some(breakpoint) = self.addr_to_breakpoints.get(&instruction_ptr) {
            // Restore original byte at breakpoint
            self.write_byte(instruction_ptr, breakpoint.orig_byte)?;
            let mut new_regs = regs;
            new_regs.rip = instruction_ptr;
            ptrace::setregs(self.pid(), new_regs)?;
            ptrace::step(self.pid(), None)?;
            let status = self.wait(None)?;
            if let Status::Stopped(signal, _) = status {
                assert!(signal == Signal::SIGTRAP);
            } else {
                return Ok(status);
            }
            self.set_breakpoint(instruction_ptr)?;
        }
        let sig = match self.pending_signal {
            Some(Signal::SIGTRAP) => None,
            x => x,
        };
        ptrace::step(self.pid(), sig)?;
        let status = self.wait(None)?;
        Ok(status)
    }

    pub fn kill(&mut self) {
        match self.child.kill() {
            Ok(_) => {
                self.wait(None).ok();
                println!("Killing running inferior (pid {})", self.pid());
            }
            Err(e) => println!("Killing running inferior failed: {}", e),
        }
    }

    pub fn print_backtrace(&self, debug_data: &DwarfData) -> Result<(), nix::Error> {
        let regs = ptrace::getregs(self.pid())?;
        let mut instruction_ptr = regs.rip;
        let mut base_ptr = regs.rbp;
        loop {
            let debug_current_line = debug_data.get_line_from_addr(instruction_ptr);
            let debug_current_func = debug_data.get_function_from_addr(instruction_ptr);
            if debug_current_line.is_none() || debug_current_func.is_none() {
                return Err(nix::Error::from(nix::errno::Errno::EINVAL));
            }
            let current_line = debug_current_line.unwrap();
            let current_func_name = debug_current_func.unwrap();
            println!(
                "{} ({}:{})",
                current_func_name, current_line.file, current_line.number
            );
            if current_func_name == "main" {
                break;
            }
            let frame_top = base_ptr + 8;
            instruction_ptr = ptrace::read(self.pid(), frame_top as ptrace::AddressType)? as u64;
            base_ptr = ptrace::read(self.pid(), base_ptr as ptrace::AddressType)? as u64;
        }
        Ok(())
    }

    pub fn set_breakpoint(&mut self, addr: u64) -> Result<u8, nix::Error> {
        let orig_byte = self.write_byte(addr, 0xcc)?;
        self.addr_to_breakpoints
            .insert(addr, Breakpoint { addr, orig_byte });
        Ok(orig_byte)
    }

    fn write_byte(&mut self, addr: u64, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(self.pid(), aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> 8 * byte_offset) & 0xff;
        let masked_word = word & !(0xff << 8 * byte_offset);
        let updated_word = masked_word | ((val as u64) << 8 * byte_offset);
        (unsafe {
            ptrace::write(
                self.pid(),
                aligned_addr as ptrace::AddressType,
                updated_word as *mut std::ffi::c_void,
            )
        })?;
        Ok(orig_byte as u8)
    }
}

use std::mem::size_of;

fn align_addr_to_word(addr: u64) -> u64 {
    addr & (-(size_of::<u64>() as i64) as u64)
}
