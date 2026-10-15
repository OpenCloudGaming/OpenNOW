# JNI control guard equivalence

Run `python3 android/scripts/test_jni_control_guards.py` from the repository root. Pass `--source /path/to/original/lib.rs` to compare a baseline, or `--rustc /path/to/rustc` to check a supported compiler.

The runner extracts the complete production `NvstBridge_keyframe` and `NvstBridge_run` functions unchanged, including their JNI declarations. Instrumented host shims check 148 combinations of attachment presence, feedback availability, stop state, Java exception-query success/failure, context extraction, transport success/error/panic and event-send failure. The assertions preserve feedback-lock lifetime, short-circuit order, notification conditions and attachment removal after every completed run, without removing another attachment.

Both original and corrected functions must pass. The regression being corrected is strict Clippy's two `collapsible_if` diagnostics; the fixture checks that changing their structure does not alter observable control behavior. No JNI runtime, Android device, transport session, thread race or physical media acceptance is exercised. The host-only JNI shims have no FFI layout guarantee and may produce compiler FFI-layout warnings; they never cross a real foreign-call boundary.

The correction deliberately avoids let-chains, which require a newer compiler than the workspace's declared Rust 1.85 minimum. It also avoids an early return after the transport completes, which would skip the required attachment cleanup.
