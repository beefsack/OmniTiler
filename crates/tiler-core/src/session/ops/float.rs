//! Session intentional-float operation: tiled-to-float and float-to-tiled transitions.
//!
//! Responsibility: own the float `impl Session` family. World state,
//! shared projection/validation helpers, transaction mechanics, and tests
//! stay in the parent session module.

use super::super::*;

impl super::super::Session {
    pub(in crate::session) fn propose_toggle_float(
        &mut self,
        window: &WindowId,
        float_geometry: Option<Rect>,
        session_observation: &SessionObservation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<SessionPlan, ProposeError> {
        if let Some(record) = self.exceptions.get(window) {
            if record.flags
                != (ExceptionFlags {
                    floating: true,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                })
            {
                return Err(ProposeError::Refused(RefusalKind::NotTiled));
            }
            let target = session_observation
                .windows
                .iter()
                .find(|entry| &entry.window == window)
                .ok_or(ProposeError::Refused(RefusalKind::PartialObservation))?;
            // Membership binds flags plus homing exactly (original
            // semantics, always used when the fixed-size opt-in is off).
            // With the opt-in on, overlays ride advisory-style and only
            // the portable floating flag binds an unfloat.
            let bindings_match = if self.fixed_admission {
                target.floating == record.flags.floating
            } else {
                target.flags() == record.flags
            };
            if !bindings_match
                || target.output != record.output
                || target.workspace != record.workspace
            {
                return Err(ProposeError::Refused(RefusalKind::PartialObservation));
            }
            // Track the live float geometry the adapter carried (the user may
            // have moved or resized the float). A request without a rect keeps
            // the already-retained placement. Fresh admission reuses the exact
            // admission placement (focused leaf's projected rect, else domain
            // bounds); the retained float rectangle is intentionally not
            // a prior-leaf restoration.
            let retained = match float_geometry {
                Some(rect) if valid_rect_shape(&rect) => Some(rect),
                Some(_) => return Err(ProposeError::Refused(RefusalKind::MalformedInput)),
                None => record
                    .floating_geometry
                    .or_else(|| self.retained_float_geometry.get(window).copied()),
            };
            // B9 maximized unfloat guard: a floating window still observed
            // maximized refuses the unfloat (pre-clear state); the adapter
            // clears the native overlay once, and the unmaximized
            // observation fresh-admits below. This holds independently of
            // the R-MAX-06 fresh-max carrier (maximized-only births admit
            // as tiles in `propose_admit`): an unfloat always requires a
            // post-clear observation.
            if target.maximized {
                return Err(ProposeError::Refused(RefusalKind::PartialObservation));
            }
            let mut candidate = self.clone();
            candidate.exceptions.remove(window);
            if let Some(rect) = retained {
                candidate
                    .retained_float_geometry
                    .insert(window.clone(), rect);
            }
            let mut observation = session_observation.clone();
            if let Some(entry) = observation
                .windows
                .iter_mut()
                .find(|entry| &entry.window == window)
            {
                entry.floating = false;
            }
            let domain = candidate
                .domain_for(&target.output, &target.workspace)
                .ok_or(ProposeError::Refused(RefusalKind::UnknownDomain))?;
            let placement = crate::seed::seed_target_bounds(&candidate, domain);
            // Explicit user-tile origin rides the admission command: the
            // fixed-size classifier cannot re-float this window, and the
            // override stages transactionally with the plan below. A
            // refusal here stages nothing, so failed proposals leave no
            // markers behind.
            let plan = candidate.propose_admit(
                window,
                &target.output,
                &target.workspace,
                ExceptionFlags::none(),
                None,
                placement,
                true,
                &observation,
                correlation_id,
                capabilities,
            )?;
            *self = candidate;
            return Ok(plan);
        }
        let target = session_observation
            .windows
            .iter()
            .find(|entry| &entry.window == window)
            .ok_or(ProposeError::Refused(RefusalKind::PartialObservation))?;
        if target.flags().any() || !self.windows.contains_key(window) {
            return Err(ProposeError::Refused(RefusalKind::NotTiled));
        }
        // Select the effective placement: an explicit rect wins, else the
        // durable retained geometry, else the centered 60% work-area fallback.
        let effective = match float_geometry {
            Some(rect) if valid_rect_shape(&rect) => rect,
            Some(_) => return Err(ProposeError::Refused(RefusalKind::MalformedInput)),
            None => match self.retained_float_geometry.get(window) {
                Some(rect) if valid_rect_shape(rect) => *rect,
                _ => centered_float_rect(
                    self.domain_for(&target.output, &target.workspace)
                        .ok_or(ProposeError::Refused(RefusalKind::UnknownDomain))?
                        .bounds,
                ),
            },
        };
        let plan =
            self.propose_remove(window, session_observation, correlation_id, capabilities)?;
        let Some(desired) = self.pending_desired.as_mut() else {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        };
        // An intentional float never shares automatic identity (D6): the
        // user chose float, so any automatic mark or tile win withdraws
        // transactionally with this plan. The later unfloat records a
        // fresh override through its own admission (D3).
        desired.automatic_fixed.remove(window);
        desired.fixed_tile_override.remove(window);
        desired.exceptions.insert(
            window.clone(),
            ExceptionRecord {
                window: window.clone(),
                output: target.output.clone(),
                workspace: target.workspace.clone(),
                flags: ExceptionFlags {
                    floating: true,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                },
                floating_geometry: Some(effective),
            },
        );
        desired
            .retained_float_geometry
            .insert(window.clone(), effective);
        // The lifecycle operation remains Remove: the only native effect is
        // removing the target from tiled actuation while its separate state is
        // committed with the same acknowledgement/verification transition.
        Ok(plan)
    }
}

/// Centered 60% work-area rectangle, matching the adapter's previous static
/// placement: `max(1, floor(0.6 * bounds))` size centered on the bounds.
pub(in crate::session) fn centered_float_rect(bounds: Rect) -> Rect {
    let w = ((i64::from(bounds.w) * 6) / 10).max(1) as i32;
    let h = ((i64::from(bounds.h) * 6) / 10).max(1) as i32;
    Rect {
        x: bounds.x + (bounds.w - w) / 2,
        y: bounds.y + (bounds.h - h) / 2,
        w,
        h,
    }
}
