# `crates/wolf-gnome/src/shell.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## One press two respawns

When the respawn keeps failing, one button press can cost two respawns. The reset restores the automatic budget, so if the button's own restart fails, `WolfCore::restart_nvim`'s `Err` arm re-marks the core `Dead` and fires a wake. `on_liveness_wake` then sees `restart_attempts == 0`, classifies as `Crashed { retrying: true }` and schedules one more automatic attempt. The user sees the message go from restart failed to restarting and back to restart failed.

This is bounded rather than a loop: the automatic attempt increments the counter again, so the sequence ends after exactly one extra respawn and only another press can start it again. Resetting the counter only on an `Alive` reading would refuse a user whose first press raced a failing automatic attempt, so the flicker is the accepted price.
