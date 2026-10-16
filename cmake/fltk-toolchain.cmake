# fltk-sys's static-msvcrt switches the C wrapper to /MT, but FLTK has its
# own runtime option. Both sides must agree with Rust's +crt-static.
set(FLTK_MSVC_RUNTIME_DLL OFF CACHE BOOL "Embed the MSVC CRT" FORCE)

# cargo-xwin/cargo-zigbuild supply a cross toolchain to the cmake crate.
# CFLTK_TOOLCHAIN selects this policy file explicitly, so chain the generated
# toolchain rather than losing its compiler, SDK headers and target platform.
string(REPLACE "-" "_" timer_target "$ENV{TARGET}")
foreach(timer_variable "CMAKE_TOOLCHAIN_FILE_${timer_target}"
                       "CMAKE_TOOLCHAIN_FILE_$ENV{TARGET}"
                       "TARGET_CMAKE_TOOLCHAIN_FILE" "CMAKE_TOOLCHAIN_FILE")
  if(DEFINED ENV{${timer_variable}} AND NOT "$ENV{${timer_variable}}" STREQUAL "${CMAKE_CURRENT_LIST_FILE}")
    include("$ENV{${timer_variable}}")
    break()
  endif()
endforeach()
