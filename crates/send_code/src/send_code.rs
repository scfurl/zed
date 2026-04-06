mod block_expander;
mod code_getter;
mod eval;
mod settings;
mod senders;

use editor::{Editor, SelectionEffects};
use gpui::{App, actions, prelude::*};
use settings::SendCodeSettings;
use ::settings::Settings;
use workspace::{Toast, Workspace, notifications::NotificationId};

pub use settings::SendCodeSettingsContent;

actions!(
    send_code,
    [
        /// Send current selection or auto-detected block to terminal, advance cursor.
        SendCode,
        /// Send current selection or auto-detected block without advancing cursor.
        SendCodeInPlace,
        /// Send the current line to terminal, advance cursor.
        SendLine,
        /// Source the entire file in the terminal.
        SendFile,
        /// Choose / cycle the send target.
        ChooseTarget,
        /// Jump cursor to the start of the next eval region.
        GotoNextEval,
        /// Jump cursor to the start of the previous eval region.
        GotoPrevEval,
    ]
);

pub fn init(cx: &mut App) {
    SendCodeSettings::register(cx);

    cx.observe_new(
        |workspace: &mut Workspace, _window, _cx: &mut Context<Workspace>| {
            workspace.register_action(|_workspace, _: &ChooseTarget, _window, _cx| {
                // TODO: open a quick-pick target switcher
                log::info!("send_code::ChooseTarget not yet implemented");
            });
        },
    )
    .detach();

    cx.observe_new(
        move |editor: &mut Editor, window, cx: &mut Context<Editor>| {
            let Some(_window) = window else {
                return;
            };

            if !editor.use_modal_editing() || !editor.buffer().read(cx).is_singleton() {
                return;
            }

            let editor_handle = cx.entity().downgrade();

            editor
                .register_action({
                    let editor_handle = editor_handle.clone();
                    move |_: &SendCode, window, cx| {
                        if !SendCodeSettings::enabled(cx) {
                            return;
                        }
                        send_code_action(editor_handle.clone(), true, window, cx);
                    }
                })
                .detach();

            editor
                .register_action({
                    let editor_handle = editor_handle.clone();
                    move |_: &SendCodeInPlace, window, cx| {
                        if !SendCodeSettings::enabled(cx) {
                            return;
                        }
                        send_code_action(editor_handle.clone(), false, window, cx);
                    }
                })
                .detach();

            editor
                .register_action({
                    let editor_handle = editor_handle.clone();
                    move |_: &SendLine, window, cx| {
                        if !SendCodeSettings::enabled(cx) {
                            return;
                        }
                        send_line_action(editor_handle.clone(), window, cx);
                    }
                })
                .detach();

            editor
                .register_action({
                    let editor_handle = editor_handle.clone();
                    move |_: &SendFile, window, cx| {
                        if !SendCodeSettings::enabled(cx) {
                            return;
                        }
                        send_file_action(editor_handle.clone(), window, cx);
                    }
                })
                .detach();

            editor
                .register_action({
                    let editor_handle = editor_handle.clone();
                    move |_: &GotoNextEval, window, cx| {
                        if !SendCodeSettings::enabled(cx) {
                            return;
                        }
                        goto_next_eval_action(editor_handle.clone(), window, cx);
                    }
                })
                .detach();

            editor
                .register_action({
                    let editor_handle = editor_handle.clone();
                    move |_: &GotoPrevEval, window, cx| {
                        if !SendCodeSettings::enabled(cx) {
                            return;
                        }
                        goto_prev_eval_action(editor_handle.clone(), window, cx);
                    }
                })
                .detach();
        },
    )
    .detach();
}

fn send_code_action(
    editor: gpui::WeakEntity<Editor>,
    advance: bool,
    window: &mut gpui::Window,
    cx: &mut App,
) {
    let Some(editor_entity) = editor.upgrade() else {
        return;
    };

    let payload = editor_entity.update(cx, |editor, cx| {
        code_getter::get_code(
            editor,
            code_getter::GetCodeMode::Auto { advance },
            cx,
        )
    });

    if let Some(payload) = payload {
        send_payload(&payload, &editor_entity, cx);

        if let Some(advance_to) = payload.advance_to {
            editor_entity.update(cx, |editor, cx| {
                editor.change_selections(SelectionEffects::default(), window, cx, |s| {
                    s.select_ranges([advance_to..advance_to]);
                });
            });
        }
    }
}

fn send_line_action(
    editor: gpui::WeakEntity<Editor>,
    window: &mut gpui::Window,
    cx: &mut App,
) {
    let Some(editor_entity) = editor.upgrade() else {
        return;
    };

    let payload = editor_entity.update(cx, |editor, cx| {
        code_getter::get_code(editor, code_getter::GetCodeMode::Line, cx)
    });

    if let Some(payload) = payload {
        send_payload(&payload, &editor_entity, cx);

        if let Some(advance_to) = payload.advance_to {
            editor_entity.update(cx, |editor, cx| {
                editor.change_selections(SelectionEffects::default(), window, cx, |s| {
                    s.select_ranges([advance_to..advance_to]);
                });
            });
        }
    }
}

fn send_file_action(
    editor: gpui::WeakEntity<Editor>,
    _window: &mut gpui::Window,
    cx: &mut App,
) {
    let Some(editor_entity) = editor.upgrade() else {
        return;
    };

    let payload = editor_entity.update(cx, |editor, cx| {
        code_getter::get_code(editor, code_getter::GetCodeMode::File, cx)
    });

    if let Some(payload) = payload {
        send_payload(&payload, &editor_entity, cx);
    }
}

fn goto_next_eval_action(
    editor: gpui::WeakEntity<Editor>,
    window: &mut gpui::Window,
    cx: &mut App,
) {
    let Some(editor_entity) = editor.upgrade() else {
        return;
    };

    editor_entity.update(cx, |editor, cx| {
        let multibuffer = editor.buffer().clone();
        let Some(buffer) = multibuffer.read(cx).as_singleton() else {
            return;
        };
        let snapshot = buffer.read(cx).snapshot();
        let display_snapshot = editor.display_snapshot(cx);
        let selection = editor.selections.newest_adjusted(&display_snapshot);
        let cursor = selection.head();

        let evals = eval::all_evals(&snapshot);
        if let Some(next) = evals.iter().find(|r| r.start > cursor) {
            let target = next.start;
            editor.change_selections(SelectionEffects::default(), window, cx, |s| {
                s.select_ranges([target..target]);
            });
        }
    });
}

fn goto_prev_eval_action(
    editor: gpui::WeakEntity<Editor>,
    window: &mut gpui::Window,
    cx: &mut App,
) {
    let Some(editor_entity) = editor.upgrade() else {
        return;
    };

    editor_entity.update(cx, |editor, cx| {
        let multibuffer = editor.buffer().clone();
        let Some(buffer) = multibuffer.read(cx).as_singleton() else {
            return;
        };
        let snapshot = buffer.read(cx).snapshot();
        let display_snapshot = editor.display_snapshot(cx);
        let selection = editor.selections.newest_adjusted(&display_snapshot);
        let cursor = selection.head();

        let evals = eval::all_evals(&snapshot);
        if let Some(prev) = evals.iter().rev().find(|r| r.end < cursor) {
            let target = prev.start;
            editor.change_selections(SelectionEffects::default(), window, cx, |s| {
                s.select_ranges([target..target]);
            });
        }
    });
}

struct SendCodeDebugToast;

fn send_payload(
    payload: &code_getter::CodePayload,
    editor: &gpui::Entity<Editor>,
    cx: &mut App,
) {
    let settings = SendCodeSettings::get_global(cx).clone();
    let language_name = payload
        .language
        .as_ref()
        .map(|l| l.name().to_string());
    let target = language_name
        .as_ref()
        .and_then(|name| settings.language_targets.get(name))
        .unwrap_or(&settings.target)
        .clone();

    let workspace = editor
        .read(cx)
        .workspace()
        .map(|ws| ws.downgrade());

    if settings.debug {
        let lang_display = language_name.as_deref().unwrap_or("(none)");
        let text_preview = if payload.text.len() > 200 {
            format!("{}…", &payload.text[..200])
        } else {
            payload.text.clone()
        };
        let msg = format!(
            "SendCode debug\nlang: {}\ntarget: {}\nbp: {}\ntext ({} chars):\n{}",
            lang_display,
            target,
            settings.bracketed_paste,
            payload.text.len(),
            text_preview,
        );
        log::info!("send_code debug: {}", msg);

        // Capture everything needed for deferred send into the on_click closure
        let text = payload.text.clone();
        let target_clone = target.clone();
        let settings_clone = settings.clone();
        let workspace_clone = workspace.clone();
        let lang_clone = language_name.clone();

        if let Some(ref ws) = workspace {
            let _ = ws.update(cx, |workspace, cx| {
                workspace.show_toast(
                    Toast::new(NotificationId::unique::<SendCodeDebugToast>(), msg)
                        .on_click("Send", move |_window, cx| {
                            senders::send_to_target(
                                &text,
                                &target_clone,
                                &settings_clone,
                                lang_clone.as_deref(),
                                workspace_clone.as_ref(),
                                cx,
                            );
                        }),
                    cx,
                );
            });
        }
        return;
    }

    senders::send_to_target(
        &payload.text,
        &target,
        &settings,
        language_name.as_deref(),
        workspace.as_ref(),
        cx,
    );
}
