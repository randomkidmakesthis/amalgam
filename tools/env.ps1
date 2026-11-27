# Portable build environment for C:\GTAVI\Tools (dot-source this from PowerShell: . C:\GTAVI\Tools\env.ps1)
$env:RUSTUP_HOME = 'C:\GTAVI\Tools\rust\rustup'
$env:CARGO_HOME  = 'C:\GTAVI\Tools\rust\cargo'
$env:PATH = "C:\GTAVI\Tools\rust\cargo\bin;$env:PATH"
$env:LIB = 'C:\GTAVI\Tools\xwin\crt\lib\x86_64;C:\GTAVI\Tools\xwin\sdk\lib\um\x86_64;C:\GTAVI\Tools\xwin\sdk\lib\ucrt\x86_64'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = 'rust-lld'
# C compiler for crates that build C code (cc-rs): clang in MSVC mode against the xwin headers
$env:CC_x86_64_pc_windows_msvc  = 'C:\GTAVI\Tools\llvm-mingw\bin\clang-cl.exe'
$env:CXX_x86_64_pc_windows_msvc = 'C:\GTAVI\Tools\llvm-mingw\bin\clang-cl.exe'
$env:AR_x86_64_pc_windows_msvc  = 'C:\GTAVI\Tools\llvm-mingw\bin\llvm-lib.exe'
$xw = 'C:/GTAVI/Tools/xwin'
$env:CFLAGS_x86_64_pc_windows_msvc = "--driver-mode=cl --target=x86_64-pc-windows-msvc /FIstring.h /imsvc$xw/crt/include /imsvc$xw/sdk/include/ucrt /imsvc$xw/sdk/include/um /imsvc$xw/sdk/include/shared"
$env:CXXFLAGS_x86_64_pc_windows_msvc = $env:CFLAGS_x86_64_pc_windows_msvc
