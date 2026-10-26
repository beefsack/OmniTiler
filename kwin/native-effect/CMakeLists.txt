cmake_minimum_required(VERSION 3.19)

project(omnitiler-native-effect LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Split build: the two settings KCMs are KWin-independent (Qt/KF6 only,
# D-Bus effect reconfigure tolerates an absent effect), while the
# active-border effect is ABI-bound to KWin headers plus the Cargo
# staticlib. Both default ON to keep the existing full developer build;
# distro/Nix settings-only consumers configure with
# -DOMNITILER_BUILD_EFFECT=OFF (needs only ECM 6.24 + Qt6/KF6),
# and effect-only companions with
# -DOMNITILER_BUILD_SETTINGS=OFF. The ECM floor is 6.24 everywhere (the
# lowest tested extra-cmake-modules) and the effect requires KWin 6.6+
# headers; the KWin script itself feature-detects the 6.7-only per-output
# workspace API at runtime and degrades to shared on 6.6.
option(OMNITILER_BUILD_EFFECT "Build the ABI-bound KWin effect" ON)
option(OMNITILER_BUILD_SETTINGS "Build the KWin-independent settings KCMs" ON)

find_package(ECM 6.24.0 REQUIRED NO_MODULE)
set(CMAKE_MODULE_PATH ${ECM_MODULE_PATH})

include(KDEInstallDirs)
include(KDECMakeSettings)
if(OMNITILER_BUILD_EFFECT)
    include(ECMSetupQtPluginMacroNames)
endif()

find_package(Qt6 REQUIRED COMPONENTS Core DBus Gui Widgets)

if(OMNITILER_BUILD_EFFECT)
    find_package(KWin 6.6.0 REQUIRED)
endif()
find_package(KF6CoreAddons REQUIRED)
find_package(KF6Config REQUIRED)
if(OMNITILER_BUILD_EFFECT)
    find_package(KF6ColorScheme REQUIRED)
endif()
find_package(KF6I18n REQUIRED)
find_package(KF6KCMUtils REQUIRED)
find_package(KF6WidgetsAddons REQUIRED)
include("${KF6I18n_DIR}/KF6I18nMacros.cmake")

if(OMNITILER_BUILD_EFFECT)
    ecm_setup_qtplugin_macro_names(
        MACRO_NAMES
            KWIN_EFFECT_FACTORY
            KWIN_EFFECT_FACTORY_ENABLED
            KWIN_EFFECT_FACTORY_SUPPORTED
            KWIN_EFFECT_FACTORY_SUPPORTED_ENABLED
    )
endif()

set(config_SOURCES)
kconfig_add_kcfg_files(config_SOURCES activeborderconfig.kcfgc)

if(OMNITILER_BUILD_EFFECT)
kcoreaddons_add_plugin(omnitiler-active-border
    INSTALL_NAMESPACE "kwin/effects/plugins"
    SOURCES
        activewindowborder.h
        activewindowborder.cpp
        gesture_lifecycle.h
        gesturefacts.h
        gesturefacts.cpp
        presentation_consumer.h
        presentation_router.h
        presentation_stage5.h
        presentation_stage5.cpp
        presentationfacts.h
        presentationfacts.cpp
        ${config_SOURCES}
)
target_link_libraries(omnitiler-active-border PRIVATE
    KWin::kwin
    KF6::ConfigCore
    KF6::ConfigGui
    KF6::ColorScheme
    Qt6::Core
    Qt6::Gui
    Qt6::DBus
)
target_compile_options(omnitiler-active-border PRIVATE -Wall -Wextra -Werror)
# Compile-version gate (causal build repair, no runtime route): KWin 6.8
# changed Effect::paintScreen from void to [[nodiscard]] bool (sibling
# v6.7.4 `src/effect/effect.h:709` vs v6.8.0 `:712`; the effect API macro
# stayed 0.237 on both sides, so no header macro fences it). The package
# version from KWinConfigVersion.cmake fences the override spelling:
# 6.7 keeps the void passthrough, 6.8 propagates the painting result.
if(KWin_VERSION VERSION_GREATER_EQUAL "6.8")
    target_compile_definitions(omnitiler-active-border PRIVATE OMNITILER_KWIN_PAINTSCREEN_BOOL)
endif()
endif()

if(OMNITILER_BUILD_SETTINGS)
set(unified_kcm_SOURCES
    unifiedsettings_module.cpp
    unifiedsettings_module.h
    kdesettings_adapter.cpp
    kdesettings_adapter.h
    shortcutreconciler.cpp
    shortcutreconciler.h
    ${config_SOURCES}
)
ki18n_wrap_ui(unified_kcm_SOURCES unifiedsettings.ui)

kcoreaddons_add_plugin(omnitiler-active-border_config
    INSTALL_NAMESPACE "kwin/effects/configs"
    SOURCES
        activeborderconfig_module.cpp
        activeborderconfig_module.h
        ${unified_kcm_SOURCES}
)
target_compile_definitions(omnitiler-active-border_config PRIVATE -DTRANSLATION_DOMAIN="kwin")
target_link_libraries(omnitiler-active-border_config PRIVATE
    KF6::CoreAddons
    KF6::ConfigCore
    KF6::ConfigGui
    KF6::I18n
    KF6::KCMUtils
    KF6::WidgetsAddons
    Qt6::Core
    Qt6::DBus
    Qt6::Widgets
)
target_compile_options(omnitiler-active-border_config PRIVATE -Wall -Wextra -Werror)

# Unified settings page: both existing plugin IDs/namespace entries display
# the identical shared QWidget page via two thin factories. Script settings
# (workspace mode, tiling gaps) live on the same page; no separate script
# page internals remain.
kcoreaddons_add_plugin(omnitiler-kwin_config
    INSTALL_NAMESPACE "kwin/scripts/configs"
    SOURCES
        scriptconfig_module.cpp
        scriptconfig_module.h
        ${unified_kcm_SOURCES}
)
target_compile_definitions(omnitiler-kwin_config PRIVATE -DTRANSLATION_DOMAIN="kwin")
target_link_libraries(omnitiler-kwin_config PRIVATE
    KF6::CoreAddons
    KF6::ConfigCore
    KF6::ConfigGui
    KF6::I18n
    KF6::KCMUtils
    KF6::WidgetsAddons
    Qt6::Core
    Qt6::DBus
    Qt6::Widgets
)
target_compile_options(omnitiler-kwin_config PRIVATE -Wall -Wextra -Werror)
endif()

include(CTest)

if(BUILD_TESTING)
    if(OMNITILER_BUILD_EFFECT)
    add_executable(omnitiler-active-border-logic-test
        activeborderlogic_test.cpp
    )
    target_link_libraries(omnitiler-active-border-logic-test PRIVATE
        Qt6::Core Qt6::Gui)
    target_compile_options(omnitiler-active-border-logic-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-logic
        COMMAND omnitiler-active-border-logic-test)

    # Read-only presentation consumer test (Stage 5, pure renderer): the
    # shared consumer header only (Qt, no KWin, no Rust staticlib). Pins
    # strict shape, generation/revision fencing, out-of-order handling, no
    # stale resurrection, underlay anchor inputs, preview publication/clear,
    # no one-shot authority, and native styling independence.
    add_executable(omnitiler-active-border-presentation-test
        presentationconsumer_test.cpp
    )
    target_link_libraries(omnitiler-active-border-presentation-test PRIVATE
        Qt6::Core)
    target_compile_options(omnitiler-active-border-presentation-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-presentation-consumer
        COMMAND omnitiler-active-border-presentation-test)

    # The private bus is provided by dbus-run-session (same package family
    # as the Qt D-Bus client already linked; no new dependency): the
    # session-bus tests below must never touch a real session bus (no KWin,
    # no live state). Located before the first use so a missing binary
    # fails the first configure instead of hiding behind a cached value.
    find_program(DBUS_RUN_SESSION dbus-run-session REQUIRED)
    # Stage 5 handler tests (Qt-only, KWin-independent): the exact
    # production PresentationStage5 handlers the effect delegates to
    # (same counter/publication/coverage/refresh code, no forwarding
    # wrapper), driven with the real router plus the real facts source
    # over a private bus and fabricated host views with stubbed surfaces.
    # Pins event-to-counter advances, the publication omission, and the
    # partial-refresh behavior. The test binary must never touch a real
    # session bus (no KWin, no live state).
    add_executable(omnitiler-active-border-stage5-test
        presentation_stage5_test.cpp
        presentation_stage5.cpp
        presentationfacts.cpp
    )
    target_link_libraries(omnitiler-active-border-stage5-test PRIVATE
        Qt6::Core
        Qt6::DBus
        Qt6::Gui)
    target_compile_options(omnitiler-active-border-stage5-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-stage5-handlers
        COMMAND ${DBUS_RUN_SESSION} -- $<TARGET_FILE:omnitiler-active-border-stage5-test>)
    set_tests_properties(native-effect-stage5-handlers PROPERTIES TIMEOUT 120)

    # Offline transport test for the versioned script facts waiter: the
    # exact production PresentationFactsSource class (no test fake) driven
    # over a real private bus in the production topology (effect-owned
    # dedicated connection plus the script's sessionBus caller). Proves
    # immediate bootstrap, change wake, idle lease, supersession,
    # foreign-caller denial without displacement, malformed/oversize
    # no-mutation, instance fencing, and shutdown invalidation; a separate
    # same-connection leg pins the Qt local-loop limit explicitly.
    add_executable(omnitiler-active-border-facts-test
        presentationfacts_test.cpp
        presentationfacts.cpp
    )
    target_link_libraries(omnitiler-active-border-facts-test PRIVATE
        Qt6::Core
        Qt6::DBus
        Qt6::Gui)
    target_compile_options(omnitiler-active-border-facts-test PRIVATE -Wall -Wextra -Werror)

    # The private bus is provided by dbus-run-session (same package family
    # as the Qt D-Bus client already linked; no new dependency): the test
    # binary must never touch a real session bus (no KWin, no live state).
    add_test(NAME native-effect-facts-transport
        COMMAND ${DBUS_RUN_SESSION} -- $<TARGET_FILE:omnitiler-active-border-facts-test>)
    set_tests_properties(native-effect-facts-transport PROPERTIES TIMEOUT 120)

    # Authoritative gesture lifecycle tests (decision B): the Qt-only store
    # plus the exact production D-Bus source over a private bus. Pins the
    # causal Esc fence (arm at the passive filter, consumed synchronously
    # by finish), fast terminal-before-pull Cancelled delivery, exact-
    # identity FIFO retention with no heuristic adoption, immutable
    # identity, abort without invented terminals, and the waiter/ack
    # transport. No KWin dispatch, no live state.
    add_executable(omnitiler-gesture-lifecycle-test
        gesturelifecycle_test.cpp
    )
    target_link_libraries(omnitiler-gesture-lifecycle-test PRIVATE
        Qt6::Core)
    target_compile_options(omnitiler-gesture-lifecycle-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-gesture-lifecycle
        COMMAND omnitiler-gesture-lifecycle-test)

    add_executable(omnitiler-gesture-facts-test
        gesturefacts_test.cpp
        gesturefacts.cpp
    )
    target_link_libraries(omnitiler-gesture-facts-test PRIVATE
        Qt6::Core
        Qt6::DBus
        Qt6::Gui)
    target_compile_options(omnitiler-gesture-facts-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-gesture-facts-transport
        COMMAND ${DBUS_RUN_SESSION} -- $<TARGET_FILE:omnitiler-gesture-facts-test>)
    set_tests_properties(native-effect-gesture-facts-transport PROPERTIES TIMEOUT 120)

    endif()

    if(OMNITILER_BUILD_SETTINGS)
    set(kcm_config_test_SOURCES
        activeborderconfig_module_test.cpp
        activeborderconfig_module.cpp
        unifiedsettings_module.cpp
        kdesettings_adapter.cpp
        shortcutreconciler.cpp
        ${config_SOURCES}
    )
    ki18n_wrap_ui(kcm_config_test_SOURCES unifiedsettings.ui)
    add_executable(omnitiler-active-border-kcm-config-test
        ${kcm_config_test_SOURCES}
    )
    target_link_libraries(omnitiler-active-border-kcm-config-test PRIVATE
        KF6::CoreAddons
        KF6::ConfigCore
        KF6::ConfigGui
        KF6::I18n
        KF6::KCMUtils
        KF6::WidgetsAddons
        Qt6::Core
        Qt6::DBus
        Qt6::Widgets
    )
    target_compile_definitions(omnitiler-active-border-kcm-config-test PRIVATE
        -DTRANSLATION_DOMAIN="kwin"
        -DKPLUGINFACTORY_PLUGIN_CLASS_INTERNAL_NAME=ActiveBorderConfigModuleFactory
    )
    target_compile_options(omnitiler-active-border-kcm-config-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-kcm-dbus
        COMMAND omnitiler-active-border-kcm-config-test dbus)
    add_test(NAME native-effect-kcm-hotapply
        COMMAND omnitiler-active-border-kcm-config-test hotapply)
    add_test(NAME native-effect-kcm-border
        COMMAND omnitiler-active-border-kcm-config-test border)
    add_test(NAME native-effect-kcm-config-reload
        COMMAND omnitiler-active-border-kcm-config-test config)
    set_tests_properties(
        native-effect-kcm-dbus
        native-effect-kcm-hotapply
        native-effect-kcm-border
        native-effect-kcm-config-reload
        PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    )

    set(kcm_shortcut_test_SOURCES
        activeborderconfig_shortcut_test.cpp
        activeborderconfig_module.cpp
        unifiedsettings_module.cpp
        kdesettings_adapter.cpp
        shortcutreconciler.cpp
        ${config_SOURCES}
    )
    ki18n_wrap_ui(kcm_shortcut_test_SOURCES unifiedsettings.ui)
    add_executable(omnitiler-active-border-kcm-shortcut-test
        ${kcm_shortcut_test_SOURCES}
    )
    target_link_libraries(omnitiler-active-border-kcm-shortcut-test PRIVATE
        KF6::CoreAddons
        KF6::ConfigCore
        KF6::ConfigGui
        KF6::I18n
        KF6::KCMUtils
        KF6::WidgetsAddons
        Qt6::Core
        Qt6::DBus
        Qt6::Widgets
    )
    target_compile_definitions(omnitiler-active-border-kcm-shortcut-test PRIVATE
        -DTRANSLATION_DOMAIN="kwin"
        -DKPLUGINFACTORY_PLUGIN_CLASS_INTERNAL_NAME=ActiveBorderConfigModuleFactory
    )
    target_compile_options(omnitiler-active-border-kcm-shortcut-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-kcm-shortcut-ordinary
        COMMAND omnitiler-active-border-kcm-shortcut-test ordinary)
    add_test(NAME native-effect-kcm-shortcut-recovery
        COMMAND omnitiler-active-border-kcm-shortcut-test recovery)
    add_test(NAME native-effect-kcm-shortcut-confirm
        COMMAND omnitiler-active-border-kcm-shortcut-test confirm)
    add_test(NAME native-effect-kcm-shortcut-state
        COMMAND omnitiler-active-border-kcm-shortcut-test state)
    add_test(NAME native-effect-kcm-shortcut-force
        COMMAND omnitiler-active-border-kcm-shortcut-test force)
    add_test(NAME native-effect-kcm-shortcut-migration
        COMMAND omnitiler-active-border-kcm-shortcut-test migration)
    add_test(NAME native-effect-kcm-shortcut-selection
        COMMAND omnitiler-active-border-kcm-shortcut-test selection)
    set_tests_properties(
        native-effect-kcm-shortcut-ordinary
        native-effect-kcm-shortcut-recovery
        native-effect-kcm-shortcut-confirm
        native-effect-kcm-shortcut-state
        native-effect-kcm-shortcut-force
        native-effect-kcm-shortcut-migration
        native-effect-kcm-shortcut-selection
        PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    )

    set(script_config_test_SOURCES
        scriptconfig_module_test.cpp
        scriptconfig_module.cpp
        unifiedsettings_module.cpp
        kdesettings_adapter.cpp
        shortcutreconciler.cpp
        ${config_SOURCES}
    )
    ki18n_wrap_ui(script_config_test_SOURCES unifiedsettings.ui)
    add_executable(omnitiler-script-config-test
        ${script_config_test_SOURCES}
    )
    target_link_libraries(omnitiler-script-config-test PRIVATE
        KF6::CoreAddons
        KF6::ConfigCore
        KF6::ConfigGui
        KF6::I18n
        KF6::KCMUtils
        KF6::WidgetsAddons
        Qt6::Core
        Qt6::DBus
        Qt6::Widgets
    )
    target_compile_definitions(omnitiler-script-config-test PRIVATE
        -DTRANSLATION_DOMAIN="kwin"
        -DKPLUGINFACTORY_PLUGIN_CLASS_INTERNAL_NAME=ScriptConfigModuleFactory
    )
    target_compile_options(omnitiler-script-config-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-script-config-dbus
        COMMAND omnitiler-script-config-test dbus)
    add_test(NAME native-script-config-gaps
        COMMAND omnitiler-script-config-test gaps)
    add_test(NAME native-script-config-save
        COMMAND omnitiler-script-config-test save)
    add_test(NAME native-script-config-windows
        COMMAND omnitiler-script-config-test windows)
    set_tests_properties(
        native-script-config-dbus
        native-script-config-gaps
        native-script-config-save
        native-script-config-windows
        PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    )

    set(descriptor_test_SOURCES
        kdesettings_adapter_test.cpp
        scriptconfig_module.cpp
        unifiedsettings_module.cpp
        kdesettings_adapter.cpp
        shortcutreconciler.cpp
        ${config_SOURCES}
    )
    ki18n_wrap_ui(descriptor_test_SOURCES unifiedsettings.ui)
    add_executable(omnitiler-settings-descriptor-test
        ${descriptor_test_SOURCES}
    )
    target_link_libraries(omnitiler-settings-descriptor-test PRIVATE
        KF6::CoreAddons
        KF6::ConfigCore
        KF6::ConfigGui
        KF6::I18n
        KF6::KCMUtils
        KF6::WidgetsAddons
        Qt6::Core
        Qt6::DBus
        Qt6::Widgets
    )
    target_compile_definitions(omnitiler-settings-descriptor-test PRIVATE
        -DTRANSLATION_DOMAIN="kwin"
        -DKPLUGINFACTORY_PLUGIN_CLASS_INTERNAL_NAME=ScriptConfigModuleFactory
    )
    target_compile_options(omnitiler-settings-descriptor-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-settings-descriptors
        COMMAND omnitiler-settings-descriptor-test descriptors)
    set_tests_properties(
        native-settings-descriptors
        PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    )

    add_test(NAME native-script-config-discovery-validation
        COMMAND ${CMAKE_COMMAND}
            -DSCRIPT_METADATA_FILE=${CMAKE_CURRENT_SOURCE_DIR}/../metadata.json
            -DSCRIPT_KCM_METADATA_FILE=${CMAKE_CURRENT_SOURCE_DIR}/scriptconfig_module.json
            -DSCRIPT_KCM_SOURCE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/scriptconfig_module.cpp
            -DSCRIPT_CMAKE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/CMakeLists.txt
            -DEXPECTED_SCRIPT_CONFIG_REFERENCE=kwin/scripts/configs/omnitiler-kwin_config
            -DEXPECTED_SCRIPT_KCM_ID=omnitiler-kwin_config
            -P ${CMAKE_CURRENT_SOURCE_DIR}/validate-scriptconfig.cmake)

    add_executable(omnitiler-shortcut-reconciler-test
        shortcutreconciler_test.cpp
        shortcutreconciler.cpp
    )
    target_link_libraries(omnitiler-shortcut-reconciler-test PRIVATE
        KF6::ConfigCore
        Qt6::Core
        Qt6::DBus
        Qt6::Gui
    )
    target_compile_options(omnitiler-shortcut-reconciler-test PRIVATE -Wall -Wextra -Werror)

    add_test(NAME native-effect-shortcut-success
        COMMAND omnitiler-shortcut-reconciler-test success)
    add_test(NAME native-effect-shortcut-conflict
        COMMAND omnitiler-shortcut-reconciler-test conflict)
    add_test(NAME native-effect-shortcut-malformed
        COMMAND omnitiler-shortcut-reconciler-test malformed)
    add_test(NAME native-effect-shortcut-owner
        COMMAND omnitiler-shortcut-reconciler-test owner)
    add_test(NAME native-effect-shortcut-recovery
        COMMAND omnitiler-shortcut-reconciler-test recovery)
    add_test(NAME native-effect-shortcut-external
        COMMAND omnitiler-shortcut-reconciler-test external)
    add_test(NAME native-effect-shortcut-journal
        COMMAND omnitiler-shortcut-reconciler-test journal)
    add_test(NAME native-effect-shortcut-force
        COMMAND omnitiler-shortcut-reconciler-test force)
    add_test(NAME native-effect-shortcut-diag
        COMMAND omnitiler-shortcut-reconciler-test diag)
    add_test(NAME native-effect-shortcut-selection
        COMMAND omnitiler-shortcut-reconciler-test selection)
    set_tests_properties(
        native-effect-shortcut-success
        native-effect-shortcut-conflict
        native-effect-shortcut-malformed
        native-effect-shortcut-owner
        native-effect-shortcut-recovery
        native-effect-shortcut-external
        native-effect-shortcut-journal
        native-effect-shortcut-force
        native-effect-shortcut-diag
        native-effect-shortcut-selection
        PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
    )

    endif()

    if(OMNITILER_BUILD_EFFECT)
    add_test(NAME native-effect-metadata-factory-validation
        COMMAND ${CMAKE_COMMAND}
            -DMETADATA_FILE=${CMAKE_CURRENT_SOURCE_DIR}/metadata.json
            -DSOURCE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.cpp
            -DKCM_METADATA_FILE=${CMAKE_CURRENT_SOURCE_DIR}/activeborderconfig_module.json
            -DKCM_SOURCE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/activeborderconfig_module.cpp
            -DPRES_HEADER=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.h
            -DPRES_IMPL=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.cpp
            -DPRES_LOGIC=${CMAKE_CURRENT_SOURCE_DIR}/activeborderlogic.h
            -DPRES_TEST=${CMAKE_CURRENT_SOURCE_DIR}/presentationconsumer_test.cpp
            -DPRES_CONSUMER=${CMAKE_CURRENT_SOURCE_DIR}/presentation_consumer.h
            -DPRES_ROUTER=${CMAKE_CURRENT_SOURCE_DIR}/presentation_router.h
            -DSTAGE_HEADER=${CMAKE_CURRENT_SOURCE_DIR}/presentation_stage5.h
            -DSTAGE_IMPL=${CMAKE_CURRENT_SOURCE_DIR}/presentation_stage5.cpp
            -DSTAGE_TEST=${CMAKE_CURRENT_SOURCE_DIR}/presentation_stage5_test.cpp
            -DDRAG_HEADER=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.h
            -DDRAG_IMPL=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.cpp
            -DEXPECTED_PLUGIN_ID=omnitiler-active-border
            -P ${CMAKE_CURRENT_SOURCE_DIR}/validate-metadata.cmake)

    add_test(NAME native-effect-unified-lifecycle
        COMMAND ${CMAKE_COMMAND}
            -DSOURCE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.cpp
            -DHEADER_FILE=${CMAKE_CURRENT_SOURCE_DIR}/activewindowborder.h
            -DCMAKE_FILE=${CMAKE_CURRENT_SOURCE_DIR}/CMakeLists.txt
            -P ${CMAKE_CURRENT_SOURCE_DIR}/validate-unified-lifecycle.cmake)
    endif()
endif()
