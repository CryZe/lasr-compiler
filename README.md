# lasr-compiler

This project provides a compiler for **LASR** (Lua Auto Splitting Runtime) scripts.

[LASR](https://github.com/LibreSplit/LibreSplit/blob/main/docs/auto-splitters.md)
is the Lua-based autosplitting runtime used by
[LibreSplit](https://github.com/LibreSplit/LibreSplit). This repository provides
a compatibility path for [LiveSplit](https://github.com/LiveSplit/LiveSplit) by
injecting LASR Lua scripts into a WASM module that runs in LiveSplit’s sandboxed
Auto Splitting Runtime.

## Download

Download prebuilt `lasr-compiler` binaries from this repository’s
[Releases](https://github.com/CryZe/lasr-compiler/releases) page.

- Linux: `lasr-compiler-linux`
- macOS: `lasr-compiler-macos`
- Windows: `lasr-compiler-windows.exe`

## Usage

Use a downloaded binary directly:

```sh
lasr-compiler script.lua [script.wasm]
```

- The second argument is optional. On Windows you can even directly drag and
  drop a Lua script onto the `lasr-compiler.exe` file to compile it.
- The resulting WASM file can be loaded into LiveSplit’s Auto Splitting Runtime.

## Compatibility

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
- `readAddress`
- `getPID`
- `print`
- `sig_scan`
- `getBaseAddress`
- `sizeOf`
- `getModuleSize`
- `getMaps`

Script settings used by the runtime loop:

- `refreshRate`
- `useGameTime`

Exclusive features of the Auto Splitting Runtime:

- `setVariable(key, var)` allows setting custom variables that can be displayed
  in LiveSplit.

Known differences and gaps:

- `mapsCacheCycles` (documented as experimental) is ignored, because the Auto
  Splitting Runtime doesn't give direct control over map caching.
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

### Workspace

- `lasr-runtime`: Lua-based autosplitting runtime compiled for `wasm32-wasip1`
- `lasr-compiler`: script compiler/injector that outputs a final runtime wasm

`lasr-compiler` embeds the runtime wasm via `build.rs`, so building/running the compiler
also builds `lasr-runtime` (release, `wasm32-wasip1`) as an input artifact.

