# About the wxp crate examples

This directory contains standalone application samples for verifying the behavior of and developing the wxp crate itself.

**Plugin developers do not need to refer to these samples.**
As a starting point for plugin development, see [`wrac-plugin-template`](https://github.com/novonotes/wrac-plugin-template/blob/main/README.md).

## List of Examples

| File | Description |
|---------|------|
| `run_loop_command_demo.rs` | Demo using the Command API with the `novonotes_run_loop` backend |
| `run_loop_channel_demo.rs` | Demo using the Channel API with the `novonotes_run_loop` backend |
| `tao_command_demo.rs` | Demo using the Command API with the `tao` backend |
| `tao_channel_demo.rs` | Demo using the Channel API with the `tao` backend |
| `winit_command_demo.rs` | Demo using the Command API with the `winit` backend |
| `winit_channel_demo.rs` | Demo using the Channel API with the `winit` backend |

These examples verify the basic behavior of Command and Channel for each of the three window backends (run_loop / tao / winit).
