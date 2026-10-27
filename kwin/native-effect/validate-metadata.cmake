if(NOT DEFINED METADATA_FILE OR NOT DEFINED SOURCE_FILE OR NOT DEFINED KCM_METADATA_FILE OR NOT DEFINED KCM_SOURCE_FILE OR NOT DEFINED EXPECTED_PLUGIN_ID)
    message(FATAL_ERROR "usage: cmake -DMETADATA_FILE=<file> -DSOURCE_FILE=<file> -DKCM_METADATA_FILE=<file> -DKCM_SOURCE_FILE=<file> -DEXPECTED_PLUGIN_ID=<id> -P validate-metadata.cmake")
endif()
if(NOT DEFINED PRES_HEADER OR NOT DEFINED PRES_IMPL OR NOT DEFINED PRES_LOGIC OR NOT DEFINED PRES_TEST)
    message(FATAL_ERROR "usage: cmake -DPRES_HEADER=<file> -DPRES_IMPL=<file> -DPRES_LOGIC=<file> -DPRES_TEST=<file> -P validate-metadata.cmake")
endif()
if(NOT DEFINED PRES_CONSUMER)
    message(FATAL_ERROR "usage: cmake -DPRES_CONSUMER=<file> -P validate-metadata.cmake")
endif()
if(NOT DEFINED PRES_ROUTER)
    message(FATAL_ERROR "usage: cmake -DPRES_ROUTER=<file> -P validate-metadata.cmake")
endif()
if(NOT DEFINED DRAG_HEADER OR NOT DEFINED DRAG_IMPL)
    message(FATAL_ERROR "usage: cmake -DDRAG_HEADER=<file> -DDRAG_IMPL=<file> -P validate-metadata.cmake")
endif()

file(READ "${METADATA_FILE}" METADATA)
file(READ "${SOURCE_FILE}" SOURCE)
file(READ "${KCM_METADATA_FILE}" KCM_METADATA)
file(READ "${KCM_SOURCE_FILE}" KCM_SOURCE)

set(REQUIRED_FIELDS Id Name Description Icon License EnabledByDefault)
foreach(FIELD IN LISTS REQUIRED_FIELDS)
    string(JSON VALUE ERROR_VARIABLE ERROR GET "${METADATA}" "KPlugin" "${FIELD}")
    if(ERROR)
        message(FATAL_ERROR "metadata validation failed: KPlugin/${FIELD} not readable: ${ERROR}")
    endif()
    if(FIELD STREQUAL "EnabledByDefault")
        if(VALUE)
            message(FATAL_ERROR "metadata validation failed: KPlugin/EnabledByDefault must be false, got '${VALUE}'")
        endif()
    elseif(VALUE STREQUAL "")
        message(FATAL_ERROR "metadata validation failed: KPlugin/${FIELD} is empty")
    endif()
endforeach()

string(JSON PLUGIN_ID ERROR_VARIABLE ERROR GET "${METADATA}" "KPlugin" "Id")
if(ERROR OR NOT PLUGIN_ID STREQUAL EXPECTED_PLUGIN_ID)
    message(FATAL_ERROR "metadata validation failed: KPlugin/Id '${PLUGIN_ID}' does not match expected '${EXPECTED_PLUGIN_ID}'")
endif()

# One plugin visibly branded OmniTiler; the stable plugin,
# KCM, and config identifiers never change.
string(JSON PLUGIN_NAME ERROR_VARIABLE ERROR GET "${METADATA}" "KPlugin" "Name")
if(ERROR OR NOT PLUGIN_NAME STREQUAL "OmniTiler")
    message(FATAL_ERROR "metadata validation failed: KPlugin/Name '${PLUGIN_NAME}' must be the visible 'OmniTiler' brand")
endif()
if(PLUGIN_ID STREQUAL "omnitiler-drag-oracle")
    message(FATAL_ERROR "metadata validation failed: the standalone drag-oracle plugin id must not survive consolidation")
endif()

string(JSON CONFIG_MODULE ERROR_VARIABLE ERROR GET "${METADATA}" "X-KDE-ConfigModule")
if(ERROR OR NOT CONFIG_MODULE STREQUAL "omnitiler-active-border_config")
    message(FATAL_ERROR "metadata validation failed: X-KDE-ConfigModule must route to the native KCM plugin")
endif()

set(REQUIRED_KCM_FIELDS Id Name Description Icon License)
foreach(FIELD IN LISTS REQUIRED_KCM_FIELDS)
    string(JSON VALUE ERROR_VARIABLE ERROR GET "${KCM_METADATA}" "KPlugin" "${FIELD}")
    if(ERROR OR VALUE STREQUAL "")
        message(FATAL_ERROR "KCM metadata validation failed: KPlugin/${FIELD} is missing or empty")
    endif()
endforeach()

string(JSON KCM_ID ERROR_VARIABLE ERROR GET "${KCM_METADATA}" "KPlugin" "Id")
if(ERROR OR NOT KCM_ID STREQUAL "omnitiler-active-border_config")
    message(FATAL_ERROR "KCM metadata validation failed: KPlugin/Id must match the configured module plugin")
endif()

string(FIND "${SOURCE}" "KWIN_EFFECT_FACTORY" FACTORY_POS)
if(FACTORY_POS EQUAL -1)
    message(FATAL_ERROR "factory validation failed: KWIN_EFFECT_FACTORY macro not found in ${SOURCE_FILE}")
endif()
string(FIND "${SOURCE}" "ActiveWindowBorderEffect" CLASS_POS)
if(CLASS_POS EQUAL -1)
    message(FATAL_ERROR "factory validation failed: ActiveWindowBorderEffect not found in ${SOURCE_FILE}")
endif()
string(FIND "${SOURCE}" "metadata.json" METADATA_REF_POS)
if(METADATA_REF_POS EQUAL -1)
    message(FATAL_ERROR "factory validation failed: metadata.json reference not found in ${SOURCE_FILE}")
endif()
string(FIND "${KCM_SOURCE}" "K_PLUGIN_CLASS_WITH_JSON" KCM_FACTORY_POS)
if(KCM_FACTORY_POS EQUAL -1)
    message(FATAL_ERROR "KCM factory validation failed: K_PLUGIN_CLASS_WITH_JSON macro not found in ${KCM_SOURCE_FILE}")
endif()
string(FIND "${KCM_SOURCE}" "activeborderconfig_module.json" KCM_METADATA_REF_POS)
if(KCM_METADATA_REF_POS EQUAL -1)
    message(FATAL_ERROR "KCM factory validation failed: embedded metadata reference not found in ${KCM_SOURCE_FILE}")
endif()

# Read-only presentation consumer (Stage 5, pure renderer): the effect owns
# the well-known service/path/interface and renders accepted core shapes
# only. The generic /Effects bus cannot
# dispatch effect-defined setters. No Rust policy owner remains for
# presentation: strict parse plus generation order fencing lives in the
# shared presentation_consumer.h (Qt only, offline-tested); the effect
# holds the paint by value, anchors it natively, and applies immediate
# hide-only vetoes. No geometry, eligibility, or policy derivation.
file(READ "${PRES_HEADER}" PRES_HEADER_TEXT)
file(READ "${PRES_IMPL}" PRES_IMPL_TEXT)
file(READ "${PRES_LOGIC}" PRES_LOGIC_TEXT)
file(READ "${PRES_TEST}" PRES_TEST_TEXT)
file(READ "${PRES_CONSUMER}" PRES_CONSUMER_TEXT)
file(READ "${PRES_ROUTER}" PRES_ROUTER_TEXT)

# Deleted superseded policy derivation must not return: no group-highlight
# or visual-policy FFI, no shared visual predicates, no outer-rect
# recomputation, no trigger/eligibility derivation. Config color accessors
# (groupUnderlayColor/dragPreviewColor/borderColor) are native styling and
# stay; the recomputation helpers below are the forbidden ones.
foreach(PRES_FORBIDDEN "group_highlight_" "visual_" "GroupHighlightState" "GroupHighlightRect" "GroupHighlightStatus" "VisualPolicyRect" "VisualPolicyBorderState" "group_highlight_ffi" "visual_policy_ffi" "groupUnderlayOuterRect" "groupUnderlayEffectiveExtension" "groupUnderlayChordHeld" "activeBorderState(" "activeBorderIsMaximized(" "activeBorderInnerRect(" "activeBorderColor(" "dragPreviewRectValid(" "activeBorderSeedMaximized")
    string(FIND "${PRES_HEADER_TEXT}" "${PRES_FORBIDDEN}" PRES_FORBIDDEN_HEADER_POS)
    if(NOT PRES_FORBIDDEN_HEADER_POS EQUAL -1)
        message(FATAL_ERROR "presentation layering validation failed: '${PRES_FORBIDDEN}' must not exist in header (superseded derivation deleted)")
    endif()
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_FORBIDDEN}" PRES_FORBIDDEN_IMPL_POS)
    if(NOT PRES_FORBIDDEN_IMPL_POS EQUAL -1)
        message(FATAL_ERROR "presentation layering validation failed: '${PRES_FORBIDDEN}' must not exist in implementation (superseded derivation deleted)")
    endif()
    string(FIND "${PRES_LOGIC_TEXT}" "${PRES_FORBIDDEN}" PRES_FORBIDDEN_LOGIC_POS)
    if(NOT PRES_FORBIDDEN_LOGIC_POS EQUAL -1)
        message(FATAL_ERROR "presentation layering validation failed: '${PRES_FORBIDDEN}' must not exist in shared logic (superseded derivation deleted)")
    endif()
endforeach()
# The legacy forwarding setters and status are deleted atomically with the
# derivation: only the presentation paint plus the retained held waiter
# remain on this endpoint.
foreach(PRES_DELETED_METHOD SetGroupHighlight ClearGroupHighlight GetGroupHighlightStatus SetDragTargetPreview ClearDragTargetPreview)
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_DELETED_METHOD}" PRES_DELETED_IMPL_POS)
    if(NOT PRES_DELETED_IMPL_POS EQUAL -1)
        message(FATAL_ERROR "presentation D-Bus validation failed: deleted ${PRES_DELETED_METHOD} must not exist")
    endif()
    string(FIND "${PRES_HEADER_TEXT}" "${PRES_DELETED_METHOD}" PRES_DELETED_HEADER_POS)
    if(NOT PRES_DELETED_HEADER_POS EQUAL -1)
        message(FATAL_ERROR "presentation D-Bus validation failed: deleted ${PRES_DELETED_METHOD} must not exist in header")
    endif()
endforeach()
foreach(PRES_CONSUMER_WIRING "applyPresentationPaint" "clearPresentationPaint" "PresentationPaint" "StaleIgnored" "Malformed")
    string(FIND "${PRES_CONSUMER_TEXT}" "${PRES_CONSUMER_WIRING}" PRES_CONSUMER_WIRING_POS)
    if(PRES_CONSUMER_WIRING_POS EQUAL -1)
        message(FATAL_ERROR "presentation wiring validation failed: '${PRES_CONSUMER_WIRING}' not found in consumer header")
    endif()
endforeach()
if(NOT DEFINED STAGE_HEADER OR NOT DEFINED STAGE_IMPL OR NOT DEFINED STAGE_TEST)
    message(FATAL_ERROR "usage: cmake -DSTAGE_HEADER=<file> -DSTAGE_IMPL=<file> -DSTAGE_TEST=<file> -P validate-metadata.cmake")
endif()
foreach(PRES_WIRING "applyPresentation" "clearPresentation" "foregroundMatchesNow" "isLiveEligibleNow" "groupMoveLiveNow" "m_router" "onTrackedFrameChanged" "onGroupMemberFrameChanged" "onWindowActivated")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_WIRING}" PRES_WIRING_IMPL_POS)
    if(PRES_WIRING_IMPL_POS EQUAL -1)
        message(FATAL_ERROR "presentation wiring validation failed: '${PRES_WIRING}' not found in implementation")
    endif()
    string(FIND "${PRES_HEADER_TEXT}" "${PRES_WIRING}" PRES_WIRING_HEADER_POS)
    if(PRES_WIRING_HEADER_POS EQUAL -1)
        message(FATAL_ERROR "presentation wiring validation failed: '${PRES_WIRING}' not found in header")
    endif()
endforeach()
# Production routing delegation: every native paint-state transition runs
# through the extracted KWin-free Stage 5 handler the offline stage test
# drives (same ownership, no second policy). The KWin handler bodies stay
# thin adapters delegating to the stage; they never reimplement
# witness/floor/latch/order logic. The stage module owns the transition
# calls on the caller's router; the router header owns the transition
# functions; the consumer header owns strict parse plus order fencing.
file(READ "${STAGE_HEADER}" STAGE_HEADER_TEXT)
file(READ "${STAGE_IMPL}" STAGE_IMPL_TEXT)
file(READ "${STAGE_TEST}" STAGE_TEST_TEXT)
foreach(PRES_ROUTER_CALL "m_router->onActivation(" "m_router->onTrackedFrame(" "m_router->onMemberFrame(" "m_router->onMotionFrame(" "m_router->onLayoutChanged(" "m_router->onVisibilityChanged(" "m_router->onGestureBoundary(" "m_router->onStreamReset(" "m_router->onDisplayHide(" "m_router->applyEndpointPaint(" "m_router->setSourceInstance(")
    string(FIND "${STAGE_IMPL_TEXT}" "${PRES_ROUTER_CALL}" PRES_ROUTER_CALL_POS)
    if(PRES_ROUTER_CALL_POS EQUAL -1)
        message(FATAL_ERROR "presentation routing validation failed: '${PRES_ROUTER_CALL}' transition missing in stage implementation")
    endif()
endforeach()
foreach(PRES_STAGE_CALL "m_stage.onActivation(" "m_stage.onTrackedFrame(" "m_stage.onMemberFrame(" "m_stage.onMotionFrame(" "m_stage.onLayoutChanged(" "m_stage.onVisibilityChanged(" "m_stage.onGestureBoundary(" "m_stage.resetStream(" "m_stage.hideForEndpointLoss(" "m_stage.applyEndpoint(" "m_stage.bindFacts(" "m_stage.decide(")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_STAGE_CALL}" PRES_STAGE_CALL_POS)
    if(PRES_STAGE_CALL_POS EQUAL -1)
        message(FATAL_ERROR "presentation routing validation failed: '${PRES_STAGE_CALL}' delegation missing in implementation")
    endif()
endforeach()
foreach(PRES_STAGE_DEF "class PresentationStage5" "Stage5Refresh onActivation" "Stage5Refresh onTrackedFrame" "Stage5Refresh onMemberFrame" "Stage5Refresh onGestureBoundary" "Stage5Refresh onVisibilityChanged" "Stage5Refresh onMemberVisibility" "applyEndpoint" "Stage5Refresh hideForEndpointLoss" "Stage5Refresh resetStream" "Stage5Visibility decide" "bindFacts" "publishInvalidation" "struct Stage5HostView" "struct Stage5Refresh" "struct Stage5Visibility")
    string(FIND "${STAGE_HEADER_TEXT}" "${PRES_STAGE_DEF}" PRES_STAGE_DEF_POS)
    if(PRES_STAGE_DEF_POS EQUAL -1)
        message(FATAL_ERROR "presentation routing validation failed: '${PRES_STAGE_DEF}' not found in stage header")
    endif()
endforeach()
foreach(PRES_STAGE_IMPL_DEF "PresentationStage5::onActivation" "PresentationStage5::onTrackedFrame" "PresentationStage5::onMemberFrame" "PresentationStage5::onGestureBoundary" "PresentationStage5::onVisibilityChanged" "PresentationStage5::onMemberVisibility" "PresentationStage5::applyEndpoint" "PresentationStage5::hideForEndpointLoss" "PresentationStage5::resetStream" "PresentationStage5::decide" "PresentationStage5::bindFacts" "PresentationStage5::publishInvalidation")
    string(FIND "${STAGE_IMPL_TEXT}" "${PRES_STAGE_IMPL_DEF}" PRES_STAGE_IMPL_DEF_POS)
    if(PRES_STAGE_IMPL_DEF_POS EQUAL -1)
        message(FATAL_ERROR "presentation routing validation failed: '${PRES_STAGE_IMPL_DEF}' not found in stage implementation")
    endif()
endforeach()
# The stage module stays KWin-free (Qt-only): no window types, no live
# scene reads, no platform includes. Host delivery arrives as injected
# inputs only.
foreach(PRES_STAGE_FORBIDDEN "EffectWindow" "windowItem" "effects->" "#include <effect/" "#include <window.h>" "#include <workspace.h>" "#include <input" "#include <scene/")
    string(FIND "${STAGE_HEADER_TEXT}" "${PRES_STAGE_FORBIDDEN}" PRES_STAGE_FORBIDDEN_HDR_POS)
    if(NOT PRES_STAGE_FORBIDDEN_HDR_POS EQUAL -1)
        message(FATAL_ERROR "presentation layering validation failed: '${PRES_STAGE_FORBIDDEN}' must not exist in stage header (KWin-independent)")
    endif()
    string(FIND "${STAGE_IMPL_TEXT}" "${PRES_STAGE_FORBIDDEN}" PRES_STAGE_FORBIDDEN_IMPL_POS)
    if(NOT PRES_STAGE_FORBIDDEN_IMPL_POS EQUAL -1)
        message(FATAL_ERROR "presentation layering validation failed: '${PRES_STAGE_FORBIDDEN}' must not exist in stage implementation (KWin-independent)")
    endif()
endforeach()
# The stage test drives the production handlers with the real router and
# the real facts source and pins publication on every invalidation advance
# plus full-surface refresh.
foreach(PRES_STAGE_COVERAGE "PresentationStage5" "PresentationRouter" "PresentationFactsSource" "bindFacts" "publishInvalidation" "onTrackedFrame" "onMemberFrame" "onGestureBoundary" "onActivation" "hideForEndpointLoss" "resetStream" "applyEndpoint")
    string(FIND "${STAGE_TEST_TEXT}" "${PRES_STAGE_COVERAGE}" PRES_STAGE_COVERAGE_POS)
    if(PRES_STAGE_COVERAGE_POS EQUAL -1)
        message(FATAL_ERROR "presentation test validation failed: '${PRES_STAGE_COVERAGE}' not covered by the stage test")
    endif()
endforeach()
foreach(PRES_ROUTER_DEF "class PresentationRouter" "void onActivation" "void onTrackedFrame" "void onMemberFrame" "void onGestureBoundary" "void onStreamReset" "void onDisplayHide" "applyEndpointPaint" "setSourceInstance")
    string(FIND "${PRES_ROUTER_TEXT}" "${PRES_ROUTER_DEF}" PRES_ROUTER_DEF_POS)
    if(PRES_ROUTER_DEF_POS EQUAL -1)
        message(FATAL_ERROR "presentation routing validation failed: '${PRES_ROUTER_DEF}' not found in router header")
    endif()
endforeach()

string(REGEX MATCHALL "Q_SCRIPTABLE" PRES_SCRIPTABLE_MATCHES "${PRES_IMPL_TEXT}")
list(LENGTH PRES_SCRIPTABLE_MATCHES PRES_SCRIPTABLE_COUNT)
if(NOT PRES_SCRIPTABLE_COUNT EQUAL 4)
    message(FATAL_ERROR "presentation D-Bus validation failed: expected paint setter, paint clear, display hide plus the held WaitPresentationFacts Q_SCRIPTABLE methods, found ${PRES_SCRIPTABLE_COUNT}")
endif()
foreach(PRES_METHOD SetPresentation ClearPresentation HidePresentation WaitPresentationFacts)
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_METHOD}" PRES_METHOD_POS)
    if(PRES_METHOD_POS EQUAL -1)
        message(FATAL_ERROR "presentation D-Bus validation failed: ${PRES_METHOD} not found")
    endif()
endforeach()
# JSON parsing lives in the shared consumer header only (offline-tested):
# the implementation holds the paint and renders, never parses.
foreach(PRES_PARSE_ONLY "QJsonDocument" "QJsonObject" "QJsonArray")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_PARSE_ONLY}" PRES_PARSE_IMPL_POS)
    if(NOT PRES_PARSE_IMPL_POS EQUAL -1)
        message(FATAL_ERROR "presentation layering validation failed: '${PRES_PARSE_ONLY}' must not exist in implementation (parse lives in the shared consumer header)")
    endif()
endforeach()
# No second geometric outset in the renderer: the core outer rects arrive
# complete and map into anchor/item space verbatim (mapFromScene only).
foreach(PRES_NO_OUTSET ".adjusted(" "adjusted(")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_NO_OUTSET}" PRES_NO_OUTSET_POS)
    if(NOT PRES_NO_OUTSET_POS EQUAL -1)
        message(FATAL_ERROR "presentation coordinate validation failed: '${PRES_NO_OUTSET}' must not exist in implementation (no second outset)")
    endif()
endforeach()
# Read-only versioned presentation facts for the script route (Stage 3
# ingress): instance/revision identity, native chord bits, actual style
# numbers, a bounded held-call lease, and an availability lane. No policy,
# no geometry writes. The waiter state machine lives in the factored
# presentationfacts module, wired to the same well-known endpoint.
file(READ "${CMAKE_CURRENT_LIST_DIR}/presentationfacts.h" GROUP_FACTS_MODULE_HEADER)
file(READ "${CMAKE_CURRENT_LIST_DIR}/presentationfacts.cpp" GROUP_FACTS_MODULE_IMPL)
foreach(PRES_FACTS "presentationFactsJson" "presentationFactsStale" "presentationFactsLeaseMs" "m_instance" "m_facts" "m_tripleKnown" "noteChord" "noteStyle" "handleWait" "completePending" "armWaiter" "isOwnerCaller")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_FACTS}" PRES_FACTS_IMPL_POS)
    string(FIND "${PRES_LOGIC_TEXT}" "${PRES_FACTS}" PRES_FACTS_LOGIC_POS)
    string(FIND "${PRES_HEADER_TEXT}" "${PRES_FACTS}" PRES_FACTS_HEADER_POS)
    string(FIND "${GROUP_FACTS_MODULE_HEADER}" "${PRES_FACTS}" PRES_FACTS_MODHDR_POS)
    string(FIND "${GROUP_FACTS_MODULE_IMPL}" "${PRES_FACTS}" PRES_FACTS_MODIMPL_POS)
    if(PRES_FACTS_IMPL_POS EQUAL -1 AND PRES_FACTS_LOGIC_POS EQUAL -1 AND PRES_FACTS_HEADER_POS EQUAL -1 AND PRES_FACTS_MODHDR_POS EQUAL -1 AND PRES_FACTS_MODIMPL_POS EQUAL -1)
        message(FATAL_ERROR "presentation facts validation failed: '${PRES_FACTS}' not found in implementation, logic, header, or facts module")
    endif()
endforeach()
foreach(PRES_FACTS_WIRE "\"v\":1" "\"instance\"" "\"rev\"" "\"invalidation\"" "\"chord\"" "\"chordKnown\"" "\"changed\"" "\"superseded\"" "\"available\"")
    string(FIND "${PRES_LOGIC_TEXT}" "${PRES_FACTS_WIRE}" PRES_FACTS_WIRE_POS)
    if(PRES_FACTS_WIRE_POS EQUAL -1)
        message(FATAL_ERROR "presentation facts validation failed: wire key ${PRES_FACTS_WIRE} not found in shared logic")
    endif()
endforeach()
foreach(PRES_FACTS_GUARD "serviceOwner" "AccessDenied" "InvalidArgs")
    string(FIND "${GROUP_FACTS_MODULE_IMPL}" "${PRES_FACTS_GUARD}" PRES_FACTS_GUARD_POS)
    if(PRES_FACTS_GUARD_POS EQUAL -1)
        message(FATAL_ERROR "presentation facts validation failed: owner gate '${PRES_FACTS_GUARD}' not found in facts module")
    endif()
endforeach()
foreach(PRES_ENDPOINT "com.omnitiler.ActiveBorder" "/com/omnitiler/ActiveBorder" "com.omnitiler.ActiveBorder1" "ExportScriptableContents" "sessionBus" "connectToBus" "disconnectFromBus" "m_groupBus" "omnitiler-active-border-endpoint" "unregisterObject" "unregisterService")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_ENDPOINT}" PRES_ENDPOINT_POS)
    if(PRES_ENDPOINT_POS EQUAL -1)
        message(FATAL_ERROR "presentation endpoint validation failed: '${PRES_ENDPOINT}' not found")
    endif()
endforeach()
string(FIND "${PRES_IMPL_TEXT}" "/Effects" PRES_EFFECTS_POS)
if(NOT PRES_EFFECTS_POS EQUAL -1)
    message(FATAL_ERROR "presentation transport validation failed: /Effects must never be used for effect-defined setters")
endif()

# Consumer contract: the shared consumer header owns strict shape plus
# order fencing; native styling stays out of the payload (independent
# colors, live system accent default, never core-derived).
foreach(PRES_CONSUMER_KEY "\"v\":1" "\"gen\"" "\"rev\"" "\"obs\"" "\"coveredInstance\"" "\"coveredInvalid\"" "\"foreground\"" "\"border\"" "\"underlay\"" "\"preview\"" "\"producerInstance\"" "\"producerSerial\"" "\"status\"" "\"members\"" "StaleIgnored" "Malformed" "16384" "isJsonIntValue" "clearPaintDisplay" "clearPaintPreview" "borderStrokeFromPublished" "presentationBorderFresh" "presentationUnderlayFresh" "presentationCoveredCurrent" "paintOrderGreater" "PaintOrder" "coverClearsFloor")
    string(FIND "${PRES_CONSUMER_TEXT}" "${PRES_CONSUMER_KEY}" PRES_CONSUMER_KEY_POS)
    if(PRES_CONSUMER_KEY_POS EQUAL -1)
        message(FATAL_ERROR "presentation consumer validation failed: '${PRES_CONSUMER_KEY}' not found in consumer header")
    endif()
endforeach()
foreach(PRES_CONSUMER_FORBIDDEN "color" "Color" "theme" "Theme" "group_highlight_" "visual_")
    string(FIND "${PRES_CONSUMER_TEXT}" "${PRES_CONSUMER_FORBIDDEN}" PRES_CONSUMER_FORBIDDEN_POS)
    if(NOT PRES_CONSUMER_FORBIDDEN_POS EQUAL -1)
        message(FATAL_ERROR "presentation consumer validation failed: '${PRES_CONSUMER_FORBIDDEN}' must not exist in consumer header (styling stays native, derivation deleted)")
    endif()
endforeach()

# Exactly one auto-lifetime outline (the preserved active border) plus the
# filled underlay below the lowest-stacked member plus the drop-preview
# overlay. No custom scene rendering. The outer rects arrive complete from
# core and map into anchor/item space verbatim (mapFromScene only).
string(REGEX MATCHALL "OutlinedBorderItem" PRES_OUTLINE_MATCHES "${PRES_HEADER_TEXT}")
list(LENGTH PRES_OUTLINE_MATCHES PRES_OUTLINE_COUNT)
if(NOT PRES_OUTLINE_COUNT EQUAL 1)
    message(FATAL_ERROR "presentation outline validation failed: expected exactly one OutlinedBorderItem member, found ${PRES_OUTLINE_COUNT}")
endif()
string(REGEX MATCHALL "ImageItem" PRES_IMAGE_MATCHES "${PRES_HEADER_TEXT}")
list(LENGTH PRES_IMAGE_MATCHES PRES_IMAGE_COUNT)
if(PRES_IMAGE_COUNT LESS 2)
    message(FATAL_ERROR "presentation underlay validation failed: expected group underlay plus drag preview ImageItem members, found ${PRES_IMAGE_COUNT}")
endif()
foreach(PRES_MEMBER "m_borderItem" "m_groupItem" "m_groupAnchor" "m_presentation" "m_dragPreviewItem")
    string(FIND "${PRES_HEADER_TEXT}" "${PRES_MEMBER}" PRES_MEMBER_POS)
    if(PRES_MEMBER_POS EQUAL -1)
        message(FATAL_ERROR "presentation outline validation failed: ${PRES_MEMBER} missing in header")
    endif()
endforeach()
foreach(PRES_RENDER "m_groupItem.setParentItem" "m_groupItem.setImage" "m_groupItem.setPosition" "m_groupItem.setSize" "m_groupItem.setVisible" "updateGroupAnchorAndGeometry" "updateGroupUnderlayFill" "stackingOrderChanged" "mapFromScene" "updateDragPreview" "m_dragPreviewItem")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_RENDER}" PRES_RENDER_POS)
    if(PRES_RENDER_POS EQUAL -1)
        message(FATAL_ERROR "presentation rendering validation failed: '${PRES_RENDER}' not found")
    endif()
endforeach()
# The underlay is filled, never an outline: no group outline calls remain.
foreach(PRES_NO_OUTLINE "m_groupItem.setOutline" "m_groupItem.setInnerRect")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_NO_OUTLINE}" PRES_NO_OUTLINE_POS)
    if(NOT PRES_NO_OUTLINE_POS EQUAL -1)
        message(FATAL_ERROR "presentation underlay validation failed: '${PRES_NO_OUTLINE}' must not exist in implementation")
    endif()
endforeach()

# Passive chord observation only (raw ingress facts): mouseChanged plus both
# modifier bits, unknown before the first signal. No polling, timers,
# grabs, filters, shortcut mutation, or paint hooks. Native code holds no
# trigger, eligibility, or visibility derivation; the held paint plus live
# vetoes decide.
foreach(PRES_PASSIVE "mouseChanged" "MetaModifier" "ShiftModifier" "m_firstMouseSeen" "m_chordHeld" "toUtf8" "internalId")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_PASSIVE}" PRES_PASSIVE_POS)
    if(PRES_PASSIVE_POS EQUAL -1)
        string(FIND "${PRES_LOGIC_TEXT}" "${PRES_PASSIVE}" PRES_PASSIVE_LOGIC_POS)
        if(PRES_PASSIVE_LOGIC_POS EQUAL -1)
            message(FATAL_ERROR "presentation visibility validation failed: '${PRES_PASSIVE}' not found")
        endif()
    endif()
endforeach()
# Focus activation must hide the old paint immediately (display-only,
# preserving order/invalidation evidence); fullscreen/minimized
# transitions must update visibility via the owned tracked signals; the
# held paint's foreground must bind to the live active internalId;
# registration failure must fail closed with no retry. Maximize observation
# seeds directly from the native committed maximizeMode() for the hide veto
# only; native transition signals stay authoritative after the seed.
foreach(PRES_BINDING "m_groupDbusAvailable" "internalId" "isInteractiveMove" "windowClosed" "clearPresentation" "maximizeMode" "observe-seed" "foregroundMatchesNow" "isLiveEligibleNow" "groupMoveLiveNow")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_BINDING}" PRES_BINDING_POS)
    if(PRES_BINDING_POS EQUAL -1)
        message(FATAL_ERROR "presentation focus/endpoint validation failed: '${PRES_BINDING}' not found")
    endif()
endforeach()
foreach(PRES_VIS "minimizedChanged" "windowFullScreenChanged" "updateGroupVisibility" "updateBorder" "updateDragPreview"
    "connect(m_trackedWindow, &EffectWindow::windowFullScreenChanged, this, &ActiveWindowBorderEffect::onTrackedFullscreenChanged)"
    "disconnect(m_trackedWindow, &EffectWindow::windowFullScreenChanged, this,"
    "m_stage.onFullscreenChanged()")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_VIS}" PRES_VIS_POS)
    if(PRES_VIS_POS EQUAL -1)
        message(FATAL_ERROR "presentation lifecycle validation failed: '${PRES_VIS}' not found")
    endif()
endforeach()
# A window removed mid-gesture must drop the held drop-preview immediately:
# forgetting the exact tracked move window clears the preview latch and
# re-evaluates the item, so no stale shape survives with no later paint to
# clear it. The latch clear plus the causal bump route through the
# production router (same ownership, tested offline); the body must not
# touch ordering state directly.
string(FIND "${PRES_IMPL_TEXT}" "void ActiveWindowBorderEffect::forgetGestureWindow" FORGET_START_POS)
if(FORGET_START_POS EQUAL -1)
    message(FATAL_ERROR "presentation removal validation failed: forgetGestureWindow missing")
endif()
string(SUBSTRING "${PRES_IMPL_TEXT}" ${FORGET_START_POS} 100000 FORGET_TAIL)
string(FIND "${FORGET_TAIL}" "\n}\n" FORGET_END_POS)
if(FORGET_END_POS EQUAL -1)
    message(FATAL_ERROR "presentation removal validation failed: forgetGestureWindow body not closed")
endif()
string(SUBSTRING "${FORGET_TAIL}" 0 ${FORGET_END_POS} FORGET_BODY)
foreach(FORGET_CLEAR "m_groupMoveWindow" "m_stage.onGestureBoundary" "applyStageRefresh")
    string(FIND "${FORGET_BODY}" "${FORGET_CLEAR}" FORGET_CLEAR_POS)
    if(FORGET_CLEAR_POS EQUAL -1)
        message(FATAL_ERROR "presentation removal validation failed: '${FORGET_CLEAR}' missing in forgetGestureWindow (held preview must clear on removal via the stage)")
    endif()
endforeach()
string(FIND "${PRES_IMPL_TEXT}" "registerService" PRES_REG_POS)
if(PRES_REG_POS EQUAL -1)
    message(FATAL_ERROR "presentation endpoint validation failed: registerService gate missing")
endif()
# The held WaitPresentationFacts call needs one bounded lease timer so a
# wedged waiter recycles; the timer lives in the factored facts module:
# single-shot, waiter-scoped, never polling, never rendering. The static
# singleShot form and framework startTimer stay forbidden.
foreach(PRES_LEASE "m_leaseTimer" "presentationFactsLeaseMs" "setSingleShot(true)" "m_leaseTimer.start")
    string(FIND "${GROUP_FACTS_MODULE_IMPL}" "${PRES_LEASE}" PRES_LEASE_MODIMPL_POS)
    if(PRES_LEASE_MODIMPL_POS EQUAL -1)
        string(FIND "${GROUP_FACTS_MODULE_HEADER}" "${PRES_LEASE}" PRES_LEASE_MODHDR_POS)
        if(PRES_LEASE_MODHDR_POS EQUAL -1)
            message(FATAL_ERROR "presentation facts lease validation failed: '${PRES_LEASE}' not found in facts module")
        endif()
    endif()
endforeach()
foreach(PRES_TIMER_FORBIDDEN "QTimer::singleShot" "startTimer")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_TIMER_FORBIDDEN}" PRES_TIMER_FORBIDDEN_POS)
    if(NOT PRES_TIMER_FORBIDDEN_POS EQUAL -1)
        message(FATAL_ERROR "presentation passivity validation failed: '${PRES_TIMER_FORBIDDEN}' must not exist in implementation")
    endif()
    string(FIND "${PRES_HEADER_TEXT}" "${PRES_TIMER_FORBIDDEN}" PRES_TIMER_FORBIDDEN_HEADER_POS)
    if(NOT PRES_TIMER_FORBIDDEN_HEADER_POS EQUAL -1)
        message(FATAL_ERROR "presentation passivity validation failed: '${PRES_TIMER_FORBIDDEN}' must not exist in header")
    endif()
endforeach()
foreach(PRES_FORBIDDEN "startMousePolling" "grabMouse" "grabKeyboard" "installEventFilter" "installSceneFilter" "registerShortcut" "registerPropertyType" "paintWindow" "drawWindow" "prePaintScreen" "postPaintScreen")
    string(FIND "${PRES_IMPL_TEXT}" "${PRES_FORBIDDEN}" PRES_FORBIDDEN_POS)
    if(NOT PRES_FORBIDDEN_POS EQUAL -1)
        message(FATAL_ERROR "presentation passivity validation failed: '${PRES_FORBIDDEN}' must not exist in implementation")
    endif()
    string(FIND "${PRES_HEADER_TEXT}" "${PRES_FORBIDDEN}" PRES_FORBIDDEN_HEADER_POS)
    if(NOT PRES_FORBIDDEN_HEADER_POS EQUAL -1)
        message(FATAL_ERROR "presentation passivity validation failed: '${PRES_FORBIDDEN}' must not exist in header")
    endif()
endforeach()

# Offline state coverage must exist through the shared consumer header and
# must poison the session bus before any Qt setup, mirroring the native
# KCM tests: strict shape, generation/revision fencing, out-of-order
# handling, no stale resurrection, underlay anchor inputs, preview
# publication/clear, no one-shot authority, native styling independence.
foreach(PRES_COVERAGE "applyPresentationPaint" "clearPresentationPaint" "StaleIgnored" "Malformed" "DBUS_SESSION_BUS_ADDRESS" "preview" "members" "underlay" "foreground" "noStyling")
    string(FIND "${PRES_TEST_TEXT}" "${PRES_COVERAGE}" PRES_COVERAGE_POS)
    if(PRES_COVERAGE_POS EQUAL -1)
        message(FATAL_ERROR "presentation test validation failed: '${PRES_COVERAGE}' not covered by the offline test")
    endif()
endforeach()

# Authoritative gesture lifecycle (essential validation): the effect hosts
# the read-only Gesture endpoint (held WaitGesture plus LastGesture reads
# and the exact-identity AckGesture retire) with the effect-owned source
# and observes only native move/resize start/step/finish geometry. No Rust
# FFI staticlib remains: the retired oracle verdict endpoint, press spy,
# and FFI record calls must not survive in the survivor.
file(READ "${DRAG_HEADER}" DRAG_HEADER_TEXT)
file(READ "${DRAG_IMPL}" DRAG_IMPL_TEXT)

foreach(GESTURE_ENDPOINT "com.omnitiler.Gesture" "/com/omnitiler/Gesture" "unregisterObject" "unregisterService")
    string(FIND "${DRAG_IMPL_TEXT}" "${GESTURE_ENDPOINT}" GESTURE_ENDPOINT_POS)
    if(GESTURE_ENDPOINT_POS EQUAL -1)
        message(FATAL_ERROR "gesture endpoint validation failed: '${GESTURE_ENDPOINT}' not found in survivor implementation")
    endif()
endforeach()
foreach(GESTURE_SIGNAL "windowStartUserMovedResized" "windowFinishUserMovedResized" "moveResizeGeometry" "gestureStarted" "gestureFinished" "gestureMoveResizeRect")
    string(FIND "${DRAG_IMPL_TEXT}" "${GESTURE_SIGNAL}" GESTURE_SIGNAL_POS)
    if(GESTURE_SIGNAL_POS EQUAL -1)
        message(FATAL_ERROR "gesture observation validation failed: '${GESTURE_SIGNAL}' not found in survivor implementation")
    endif()
endforeach()
foreach(RETIRED_ORACLE_SYMBOL "LastVerdict" "com.omnitiler.DragOracle" "drag_oracle_record" "drag_oracle_last_copy" "OraclePressSpy" "OraclePressCandidate")
    string(FIND "${DRAG_IMPL_TEXT}" "${RETIRED_ORACLE_SYMBOL}" RETIRED_IMPL_POS)
    if(NOT RETIRED_IMPL_POS EQUAL -1)
        message(FATAL_ERROR "gesture cutover validation failed: retired oracle symbol '${RETIRED_ORACLE_SYMBOL}' must not remain in survivor implementation")
    endif()
    string(FIND "${DRAG_HEADER_TEXT}" "${RETIRED_ORACLE_SYMBOL}" RETIRED_HEADER_POS)
    if(NOT RETIRED_HEADER_POS EQUAL -1)
        message(FATAL_ERROR "gesture cutover validation failed: retired oracle symbol '${RETIRED_ORACLE_SYMBOL}' must not remain in survivor header")
    endif()
endforeach()

message(STATUS "metadata/factory validation passed for ${METADATA_FILE} and ${KCM_METADATA_FILE}")
