cmake_minimum_required(VERSION 3.20)

project(driver_virtual_hmd LANGUAGES CXX)

add_library(driver_virtual_hmd SHARED
    src/driver_factory.cpp
    src/device_provider.cpp
    src/virtual_hmd_device.cpp
    src/pose_pipe.cpp
    src/log.cpp
)

target_compile_features(driver_virtual_hmd PRIVATE cxx_std_17)

target_include_directories(driver_virtual_hmd PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/src
    ${CMAKE_CURRENT_SOURCE_DIR}/../third_party/openvr
)

target_compile_definitions(driver_virtual_hmd PRIVATE
    WIN32_LEAN_AND_MEAN
    NOMINMAX
)

if(MSVC)
    target_compile_options(driver_virtual_hmd PRIVATE /W4 /permissive-)
endif()

