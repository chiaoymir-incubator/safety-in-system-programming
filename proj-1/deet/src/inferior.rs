use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::process::Child;
use std::process::Command;
use std::os::unix::process::CommandExt;
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use std::mem::size_of;
use std::collections::HashMap;

fn align_addr_to_word(addr: usize) -> usize {
    addr & (-(size_of::<usize>() as isize) as usize)
}

#[derive(Clone)]
struct Breakpoint {
    addr: usize,
    orig_byte: u8,
}

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, usize),

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

pub struct Inferior {
    child: Child,
    break_map: HashMap<usize, Breakpoint>,
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>, breakpoints: &Vec<usize>) -> Option<Inferior> {
        let mut cmd = Command::new(target);// .args(args);
        unsafe {
            cmd.pre_exec(child_traceme);
        }
        let child = cmd.args(args).spawn().ok()?;
        let child_id = nix::unistd::Pid::from_raw(child.id() as i32);

        match waitpid(child_id, Some(WaitPidFlag::WNOHANG)).ok()? {
            WaitStatus::Stopped(_pid, _signal) => (),
            _ => return None,
        }

        let mut inferior = Inferior { 
            child: child, 
            break_map: HashMap::new(),
        };
        // install breakpoints 
        for b in breakpoints {
            inferior.add_breakpoint(*b);
        }

        Some( inferior )
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }

    pub fn cont(&mut self) -> Result<Status, nix::Error> {
        let mut regs = ptrace::getregs(self.pid())?;
        let rip = regs.rip as usize;
        // is stopped as breakpoint
        if self.break_map.contains_key(&(rip - 1)) {
            println!("rip: {:#x}", rip);
            ptrace::step(self.pid(), None);

            // wait for SIGTRAP.
            // if the process terminates, just return
            match self.wait(None) {
                Ok(status) => {
                    match status {
                        Status::Stopped(_sig, _rip) => (),
                        other => return Ok(other),
                    }
                },
                err => return err,
            }

            self.write_byte(rip, 0xcc);
        }

        ptrace::cont(self.pid(), None)?;
        let result = self.wait(None);

        // if stopped at a breakpoint
        let mut regs = ptrace::getregs(self.pid())?;
        let rip = regs.rip as usize;
        if self.break_map.contains_key(&(rip - 1)) {
            let breakpoint = &self.break_map[&(rip - 1)];
            println!("breakpoint: {:#x}", breakpoint.addr);
            self.write_byte(breakpoint.addr, breakpoint.orig_byte);
            // rewind rip by 1 to the breakpoint
            regs.rip -= 1;
            ptrace::setregs(self.pid(), regs)?;
        }
        result
    }

    pub fn kill(&mut self) -> bool {
        println!("Killing running inferior (pid {})", self.pid());
        match Child::kill(&mut self.child) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    pub fn print_location(&self, debug_data: &DwarfData, rip: usize) {
        let line_option = debug_data.get_line_from_addr(rip);
        let func_name_option = debug_data.get_function_from_addr(rip);
        if !line_option.is_none() && !func_name_option.is_none() {
            let func_name = func_name_option.unwrap();
            let line = line_option.unwrap();
            println!("{} ({}:{})", func_name, line.file, line.number);
        }
    }

    pub fn print_backtrace(&self, debug_data: &DwarfData) -> Result<(), nix::Error> {
        let regs = ptrace::getregs(self.pid())?;
        let mut rip = regs.rip as usize;
        let mut rbp = regs.rbp as usize;
        loop {
            let line = debug_data.get_line_from_addr(rip).unwrap();
            let func_name = debug_data.get_function_from_addr(rip).unwrap();
            println!("{} ({}:{})", func_name, line.file, line.number);
            if func_name == "main" { 
                break ;
            }
            rip = ptrace::read(self.pid(), (rbp + 8) as ptrace::AddressType)? as usize;
            rbp = ptrace::read(self.pid(), rbp as ptrace::AddressType)? as usize;
        }
        Ok(())
    }

    pub fn add_breakpoint(&mut self, addr: usize) -> Result<u8, nix::Error> {
        // (0xcc causes SIGTRAP)
        self.write_byte(addr, 0xcc)
    }

    fn write_byte(&mut self, addr: usize, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(self.pid(), aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> 8 * byte_offset) & 0xff;
        let masked_word = word & !(0xff << 8 * byte_offset);
        let updated_word = masked_word | ((val as u64) << 8 * byte_offset);
        ptrace::write(
            self.pid(),
            aligned_addr as ptrace::AddressType,
            updated_word as *mut std::ffi::c_void,
        )?;
        // add the breakpoint to the map
        self.break_map.insert(addr, Breakpoint{
            addr: addr, 
            orig_byte: orig_byte as u8,
        });
        Ok(orig_byte as u8)
    }
}
