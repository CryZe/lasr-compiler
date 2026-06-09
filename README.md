# lasr-compiler

This project provides a compiler for **LASR** (Lua Auto Splitting Runtime) scripts.

[LASR](https://github.com/LibreSplit/LibreSplit/blob/main/docs/auto-splitters.md)
is the Lua-based autosplitting runtime used by
[LibreSplit](https://github.com/LibreSplit/LibreSplit). This repository provides
a compatibility path for [LiveSplit](https://github.com/LiveSplit/LiveSplit) by
injecting LASR Lua scripts into a WASM module that runs in LiveSplit’s sandboxed
Auto Splitting Runtime.

## Download

Download the prebuilt `lasr-compiler` binaries here:

- [Linux](https://github.com/CryZe/lasr-compiler/releases/download/latest/lasr-compiler-linux.tar.gz)
- [macOS](https://github.com/CryZe/lasr-compiler/releases/download/latest/lasr-compiler-macos.tar.gz)
- [Windows](https://github.com/CryZe/lasr-compiler/releases/download/latest/lasr-compiler-windows.zip)

## Usage

Use a downloaded binary directly:

```sh
lasr-compiler script.lua [script.wasm]
```

- The second argument is optional. On Windows you can even directly drag and
  drop a Lua script onto the `lasr-compiler.exe` file to compile it.
- The resulting WASM file can be loaded into LiveSplit’s Auto Splitting Runtime.

## Compatibility

The current compatibility target is LibreSplit commit [eb95cea](https://github.com/LibreSplit/LibreSplit/commit/eb95cea).

Callback lifecycle:

- `startup`
- `state`
- `update`
- `start`
- `split`
- `isLoading`
- `reset`
- `gameTime`

Lua globals / host functions:

- `process`
- `cmdline`
- `readAddress`
- `getPID`
- `print`
- `print_tbl`
- `sig_scan`
- `getBaseAddress`
- `sizeOf`
- `getModuleSize`
- `getMaps`
- `str2ida`
- `shallow_copy_tbl`
- `md5sum`

Script settings used by the runtime loop:

- `refreshRate`
- `useGameTime`
- `mapsCacheCycles`

Exclusive features of the Auto Splitting Runtime:

- `setVariable(key, var)` allows setting custom variables that can be displayed
  in LiveSplit.

Known differences and gaps:

- `cmdline` currently falls back to executable-name matching. The Auto
  Splitting Runtime does not expose full command-line process matching.
- `getPID` currently returns a dummy value (`0`) because the Auto Splitting
  Runtime does not implement process ID retrieval.
- `getMaps` currently returns an empty `name` field for each map because the
  Auto Splitting Runtime does not implement map name retrieval.
- The Lua stdlib is not fully supported and may behave differently due to the
  sandboxed environment.

Please create an issue if you find any incompatibilities or missing features
that affect you.

## Development

If you want to build from source, [install
Rust](https://www.rust-lang.org/tools/install) and then the WASI target:

```sh
rustup target add wasm32-wasip1 --toolchain stable
```

Build compiler:

```sh
cargo build
```

Build runtime only (not usually necessary):

```sh
cargo build -p lasr-runtime --target wasm32-wasip1
```

