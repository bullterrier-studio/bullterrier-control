//! The memory cap of this crate's test binary: every allocation goes through [`cap::Cap`], which refuses one that
//! would take the binary past [`CAP`]; the refusal aborts the process ("memory allocation of N bytes failed"), so a
//! runaway test stops at once instead of filling the machine's memory. A bare `cargo test` is capped, with no setting;
//! doctests are binaries of their own and are not. The `unsafe` of an allocator lives in the `cap` crate: this crate
//! keeps `forbid(unsafe_code)`.

use std::alloc::System;
use std::process::Command;

use cap::Cap;

/// The most the test binary may hold at once, in bytes: 4 GiB, about 30 times what a whole run holds at its peak
/// (about 140 MB).
const CAP: usize = 4 << 30;

#[global_allocator]
static ALLOCATOR: Cap<System> = Cap::new(System, CAP);

/// The variable that makes [`allocates_past_a_cap_lowered_to_the_mib_asked`] run: the cap it sets, in MiB.
const CHILD_CAP: &str = "BT_CONTROL_TEST_CHILD_CAP_MIB";

/// The child of [`an_allocation_past_the_cap_aborts_the_test_binary`]: inert unless [`CHILD_CAP`] is set, which only
/// that test sets, in the child's environment alone. It asks for twice the cap at once: refused before any memory is
/// taken.
#[test]
fn allocates_past_a_cap_lowered_to_the_mib_asked() -> Result<(), std::num::ParseIntError> {
    let Ok(mib) = std::env::var(CHILD_CAP) else {
        return Ok(());
    };
    let cap = mib.parse::<usize>()? << 20;
    assert!(
        ALLOCATOR.set_limit(cap).is_ok(),
        "the binary holds more than {mib} MiB already"
    );
    std::hint::black_box(Vec::<u8>::with_capacity(2 * cap));
    Ok(())
}

#[test]
fn an_allocation_past_the_cap_aborts_the_test_binary() -> std::io::Result<()> {
    for (row, cap, aborts, says) in [
        ("inert without the variable", None, false, "1 passed"),
        (
            "past a cap of 64 MiB",
            Some("64"),
            true,
            "memory allocation of 134217728 bytes failed",
        ),
    ] {
        let mut child = Command::new(std::env::current_exe()?);
        child.args([
            "--exact",
            "memory_cap::allocates_past_a_cap_lowered_to_the_mib_asked",
            "--test-threads=1",
        ]);
        match cap {
            Some(mib) => child.env(CHILD_CAP, mib),
            None => child.env_remove(CHILD_CAP),
        };
        let output = child.output()?;
        let text = String::from_utf8_lossy(&[output.stdout, output.stderr].concat()).into_owned();
        assert_eq!(!output.status.success(), aborts, "{row}: {}: {text}", output.status);
        assert!(text.contains(says), "{row}: {text}");
    }
    Ok(())
}
