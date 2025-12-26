# DEET Debugger

**Course:** System Programming 2025  
**Project:** The DEET Debugger  
**Reference:** CS 110L: Safety in Systems Programming - Project 1  
([Assignment Spec](https://reberhardt.com/cs110l/spring-2020/assignments/project-1/) | [Starter Code](https://github.com/reberhardt7/cs110l-spr-2020-starter-code/tree/main/proj-1))

## Overview

> **Note:** This project is designed for **Linux** only, as it relies on Linux-specific system calls like `ptrace`.

DEET (Debugs Executables Extremely Terribly) is a lightweight, GDB-like debugger implemented in Rust. It allows users to inspect and control the execution of a target program using standard debugging features like breakpoints, stepping, and variable inspection.

This project demonstrates core system programming concepts including:
- **Process Control:** Using `fork`, `exec`, and `waitpid` to manage inferior processes.
- **Ptrace API:** Leveraging `ptrace` to intercept signals, read/write memory, and control execution flow.
- **DWARF Debugging Information:** Parsing DWARF data to map machine code addresses to source code lines, functions, and variables.
- **Signal Handling:** Managing signals like `SIGTRAP` and `SIGINT` to coordinate between the debugger and the debuggee.

## Features

- **Process Management:**
  - Start a new process (`run`)
  - Continue execution (`continue`)
  - Kill the running process (`quit`)

- **Execution Control:**
  - **Breakpoints:** Set breakpoints by function name, line number, or raw address (`break`).
  - **Stepping:** Step through the code line-by-line (`step` or `next`).
  - **Prologue Skipping:** Automatically detects function prologues and stops at the first line of user code, ensuring stack frames are set up correctly (similar to GDB).

- **Inspection:**
  - **Backtrace:** Print the current call stack (`backtrace`).
  - **Variable Inspection:** Print the value of variables in the current scope (`print`). Supports global variables and local variables (via stack frame offsets).
  - **Source Listing:** Displays the current source line when stopped.

## Usage

### Building

To build the debugger:

```bash
cargo build
```

To build the sample programs for debugging:

```bash
make
```

### Running

To run DEET on a target executable (e.g., `samples/segfault`):

```bash
cargo run -- samples/segfault
```

### Commands

Once inside the DEET prompt `(deet)`, you can use the following commands:

| Command | Alias | Description |
|---------|-------|-------------|
| `run [args]` | `r` | Start (or restart) the target program with optional arguments. |
| `continue` | `c`, `cont` | Continue execution until the next breakpoint or signal. |
| `step [n]` | `s` | Execute the next line of source code. Optional `n` steps multiple lines. |
| `breakpoint <loc>` | `b`, `break` | Set a breakpoint. `<loc>` can be a function name (`main`), line number (`10`), or address (`*0x4005b6`). |
| `print <var>` | `p` | Print the value of a variable. |
| `backtrace` | `bt`, `back` | Show the current call stack. |
| `quit` | `q` | Exit the debugger. |

### Example Session

```text
cargo run -- samples/segfault
(deet) b func1
Setting breakpoint 0 at 0x4011a9
(deet) r
Child stopped (signal SIGTRAP)
Stopped at func1 (/path/to/deet/samples/segfault.c:9)
9       void func1(int a) {
(deet) s
Child stopped (signal SIGTRAP)
Stopped at func1 (/path/to/deet/samples/segfault.c:10)
10      printf("Calling func2\n");
(deet) p a
Found variable a (int 4, located at FramePointerOffset(-20), declared at line 9) in function func1
a = 42
(deet) c
Calling func2
About to segfault... a=2
Child stopped (signal SIGSEGV)
Stopped at func2 (/path/to/deet/samples/segfault.c:5)
5       *(int*)0 = a;
(deet) bt
func2 (/path/to/deet/samples/segfault.c:5)
func1 (/path/to/deet/samples/segfault.c:11)
main (/path/to/deet/samples/segfault.c:15)
(deet) p a
Found variable a (int 4, located at FramePointerOffset(-20), declared at line 3) in function func2
a = 2
(deet) s
Child terminated with signal SIGSEGV
(deet) q
```

## Implementation Details

- **Inferior Management:** The `Inferior` struct wraps the child process, handling `ptrace` calls and status updates.
- **Breakpoint Handling:** Breakpoints are implemented by writing the `0xcc` (INT 3) instruction to memory. When hit, the original instruction is restored, the instruction pointer is decremented, and execution resumes.
- **DWARF Parsing:** Uses the `gimli` crate to parse debug info. Custom logic was added to `DwarfData` to correctly resolve function entry points and skip prologues, ensuring variables are accessible when execution stops.
- **Variable Printing:** Resolves variable locations (stack offsets or absolute addresses) using DWARF data and reads memory via `ptrace`.

## Acknowledgements

This project is based on the CS 110L course at Stanford University.
