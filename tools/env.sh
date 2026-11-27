# Portable build environment for C:\GTAVI\Tools (source this from Git Bash)
export RUSTUP_HOME='C:\GTAVI\Tools\rust\rustup'
export CARGO_HOME='C:\GTAVI\Tools\rust\cargo'
export PATH="/c/GTAVI/Tools/rust/cargo/bin:$PATH"
export LIB='C:\GTAVI\Tools\xwin\crt\lib\x86_64;C:\GTAVI\Tools\xwin\sdk\lib\um\x86_64;C:\GTAVI\Tools\xwin\sdk\lib\ucrt\x86_64'
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=rust-lld
# C compiler for crates that build C code (cc-rs): clang in MSVC mode against the xwin headers
export CC_x86_64_pc_windows_msvc='C:\GTAVI\Tools\llvm-mingw\bin\clang-cl.exe'
export CXX_x86_64_pc_windows_msvc='C:\GTAVI\Tools\llvm-mingw\bin\clang-cl.exe'
export AR_x86_64_pc_windows_msvc='C:\GTAVI\Tools\llvm-mingw\bin\llvm-lib.exe'
_XW='C:/GTAVI/Tools/xwin'
export CFLAGS_x86_64_pc_windows_msvc="--driver-mode=cl --target=x86_64-pc-windows-msvc /FIstring.h /imsvc$_XW/crt/include /imsvc$_XW/sdk/include/ucrt /imsvc$_XW/sdk/include/um /imsvc$_XW/sdk/include/shared"
export CXXFLAGS_x86_64_pc_windows_msvc="$CFLAGS_x86_64_pc_windows_msvc"
