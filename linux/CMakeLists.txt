cmake_minimum_required(VERSION 3.20)
project(DrinkKDE LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
add_compile_options(-Wall -Wextra -Werror)

find_package(Qt6 REQUIRED COMPONENTS Widgets DBus)
find_package(LayerShellQt 6.6 REQUIRED)
find_package(KF6Service REQUIRED)
find_package(KF6StatusNotifierItem 6.14 REQUIRED)
find_package(PkgConfig REQUIRED)
pkg_check_modules(PipeWire REQUIRED IMPORTED_TARGET libpipewire-0.3)

# Both native C++ adapters consume the same generated interface. Let Cargo
# handle incremental rebuilds of the scheduler, bridge and locked dependencies.
set(cargo_target "${CMAKE_BINARY_DIR}/cargo")
set(bridge "${cargo_target}/release/libdrink_macos_bridge.a")
add_custom_target(scheduler
    COMMAND cargo build --locked --release -p drink-macos-bridge --target-dir "${cargo_target}"
    WORKING_DIRECTORY "${PROJECT_SOURCE_DIR}/.."
    BYPRODUCTS "${bridge}" "${cargo_target}/cxxbridge/drink-macos-bridge/src/lib.rs.h"
    VERBATIM)

add_library(drink-audio STATIC CaptureGraph.cpp Microphone.cpp)
target_link_libraries(drink-audio PRIVATE PkgConfig::PipeWire)
target_include_directories(drink-audio PUBLIC "${PROJECT_SOURCE_DIR}")

add_library(drink-plasma STATIC kde/BreakWindows.cpp kde/Drink.cpp kde/Foreground.cpp kde/Session.cpp)
set_target_properties(drink-plasma PROPERTIES AUTOMOC ON)
add_dependencies(drink-plasma scheduler)
target_include_directories(drink-plasma PUBLIC kde "${cargo_target}/cxxbridge")
target_link_libraries(drink-plasma PUBLIC drink-audio Qt6::Widgets Qt6::DBus KF6::StatusNotifierItem
    PRIVATE LayerShellQt::Interface KF6::Service "${bridge}" ${CMAKE_DL_LIBS})

add_executable(drink-kde kde/main.cpp kde/resources.qrc)
set_target_properties(drink-kde PROPERTIES AUTORCC ON INSTALL_RPATH_USE_LINK_PATH ON)
target_link_libraries(drink-kde PRIVATE drink-plasma)
include(GNUInstallDirs)
install(TARGETS drink-kde RUNTIME DESTINATION "${CMAKE_INSTALL_BINDIR}")
install(FILES kde/io.github.napolitain.Drink.desktop DESTINATION "${CMAKE_INSTALL_DATADIR}/applications")
install(FILES kde/drink.svg DESTINATION "${CMAKE_INSTALL_DATADIR}/icons/hicolor/scalable/apps"
    RENAME io.github.napolitain.Drink.svg)

include(CTest)
if(BUILD_TESTING)
    add_executable(capture-test tests/capture.cpp)
    target_link_libraries(capture-test PRIVATE drink-audio)
    add_test(NAME capture-graph COMMAND capture-test)
    find_package(Qt6 REQUIRED COMPONENTS Test)
    add_executable(plasma-test tests/plasma.cpp kde/resources.qrc)
    set_target_properties(plasma-test PROPERTIES AUTOMOC ON AUTORCC ON)
    target_link_libraries(plasma-test PRIVATE drink-plasma Qt6::Test)
endif()
