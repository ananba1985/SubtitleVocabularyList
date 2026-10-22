//! Thin Windows menu; every selection uses the existing libmpv control path.
use super::{PlayerSnapshot, WatchControl};
use crate::error::AppError;
use windows::{
    Win32::{
        Foundation::{HWND, POINT},
        UI::WindowsAndMessaging::*,
    },
    core::PCWSTR,
};

struct Item {
    title: String,
    action: Option<WatchControl>,
    children: Vec<Item>,
    checked: bool,
    disabled: bool,
}
fn action(title: impl Into<String>, name: &str, value: Option<f64>, text: Option<&str>) -> Item {
    Item {
        title: title.into(),
        action: Some(WatchControl {
            action: name.into(),
            value,
            text: text.map(String::from),
            expected_revision: None,
        }),
        children: vec![],
        checked: false,
        disabled: false,
    }
}
fn checked(mut item: Item, value: bool) -> Item {
    item.checked = value;
    item
}
fn group(title: impl Into<String>, children: Vec<Item>) -> Item {
    Item {
        title: title.into(),
        action: None,
        disabled: children.is_empty(),
        children,
        checked: false,
    }
}
fn tracks(state: &PlayerSnapshot, kind: &str, name: &str, secondary: bool) -> Vec<Item> {
    let selected = state
        .tracks
        .iter()
        .any(|t| t.kind == kind && if secondary { t.secondary } else { t.selected });
    let mut items = vec![checked(action("关闭", name, None, None), !selected)];
    for track in state.tracks.iter().filter(|t| t.kind == kind) {
        items.push(checked(
            action(
                format!("{} · {}", track.id, track.title),
                name,
                Some(track.id as f64),
                None,
            ),
            if secondary {
                track.secondary
            } else {
                track.selected
            },
        ));
    }
    items
}
fn items(s: &PlayerSnapshot, fullscreen: bool) -> Vec<Item> {
    let p = &s.playback.preferences;
    let speed = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 3.0, 4.0]
        .into_iter()
        .map(|v| {
            checked(
                action(format!("{v}×"), "speed", Some(v), None),
                (p.speed - v).abs() < 0.001,
            )
        })
        .collect();
    let size = [0.75, 1.0, 1.25, 1.5, 2.0, 3.0]
        .into_iter()
        .map(|v| {
            checked(
                action(
                    format!("{}%", (v * 100.0) as u32),
                    "subtitle_scale",
                    Some(v),
                    None,
                ),
                (p.subtitle_scale - v).abs() < 0.001,
            )
        })
        .collect();
    let position = [
        ("顶部", 0.0),
        ("偏上", 25.0),
        ("居中", 50.0),
        ("偏下", 75.0),
        ("底部", 100.0),
    ]
    .into_iter()
    .map(|(label, v)| {
        checked(
            action(label, "subtitle_position", Some(v), None),
            (p.subtitle_position - v).abs() < 0.01,
        )
    })
    .collect();
    let chapters = s
        .chapters
        .iter()
        .map(|c| {
            checked(
                action(
                    format!("{} · {}", c.index + 1, c.title),
                    "chapter",
                    Some(c.index as f64),
                    None,
                ),
                s.chapter_index == Some(c.index),
            )
        })
        .collect();
    vec![
        action(
            if s.playing {
                "暂停\tSpace"
            } else {
                "播放\tSpace"
            },
            "toggle",
            None,
            None,
        ),
        action("停止播放", "close", None, None),
        group(
            "剧集",
            vec![
                action("上一集", "playlist_previous", None, None),
                action("下一集", "playlist_next", None, None),
                checked(
                    action(
                        "自动连播",
                        "auto_next",
                        Some(if p.auto_next { 0.0 } else { 1.0 }),
                        None,
                    ),
                    p.auto_next,
                ),
            ],
        ),
        group(format!("倍速（{:.2}×）", p.speed), speed),
        group(
            "声音",
            vec![
                checked(
                    action(
                        "静音\tm",
                        "mute",
                        Some(if p.muted { 0.0 } else { 1.0 }),
                        None,
                    ),
                    p.muted,
                ),
                group("音轨", tracks(s, "audio", "audio", false)),
            ],
        ),
        group(
            "字幕",
            vec![
                group("主字幕", tracks(s, "sub", "subtitle", false)),
                group("第二字幕", tracks(s, "sub", "secondary_subtitle", true)),
                action("加载外置字幕…", "subtitle_dialog", None, None),
                checked(
                    action(
                        "显示主字幕\tv",
                        "subtitle_visible",
                        Some(if p.subtitle_visible { 0.0 } else { 1.0 }),
                        None,
                    ),
                    p.subtitle_visible,
                ),
                checked(
                    action(
                        "显示第二字幕",
                        "secondary_subtitle_visible",
                        Some(if p.secondary_subtitle_visible {
                            0.0
                        } else {
                            1.0
                        }),
                        None,
                    ),
                    p.secondary_subtitle_visible,
                ),
                group(
                    format!("字幕延迟（{:.1} 秒）", s.subtitle_delay),
                    vec![
                        action(
                            "提前 0.1 秒",
                            "subtitle_delay",
                            Some(s.subtitle_delay - 0.1),
                            None,
                        ),
                        action(
                            "延后 0.1 秒",
                            "subtitle_delay",
                            Some(s.subtitle_delay + 0.1),
                            None,
                        ),
                        action("复位", "subtitle_delay", Some(0.0), None),
                    ],
                ),
                group(
                    format!("字号（{}%）", (p.subtitle_scale * 100.0).round() as u32),
                    size,
                ),
                group("位置", position),
            ],
        ),
        group(
            "定位",
            vec![
                action("跳到指定时间…", "seek_prompt", None, None),
                action("后退重看 10 秒", "seek_relative", Some(-10.0), None),
                action("前进 10 秒", "seek_relative", Some(10.0), None),
                action("重播当前句", "subtitle_seek", Some(0.0), None),
                action("上一句\tCtrl+←", "subtitle_seek", Some(-1.0), None),
                action("下一句\tCtrl+→", "subtitle_seek", Some(1.0), None),
                group("章节", chapters),
            ],
        ),
        group(
            "循环",
            vec![
                checked(
                    action("关闭循环", "loop_mode", None, Some("none")),
                    p.loop_mode == "none",
                ),
                checked(
                    action("单集循环", "loop_mode", None, Some("file")),
                    p.loop_mode == "file",
                ),
                checked(
                    action("列表循环", "loop_mode", None, Some("playlist")),
                    p.loop_mode == "playlist",
                ),
                action("设置当前点为 A", "loop_a", None, None),
                action("设置当前点为 B", "loop_b", None, None),
                action("清除 A–B 区间", "loop_clear", None, None),
            ],
        ),
        action(
            if fullscreen {
                "退出全屏\tEsc"
            } else {
                "全屏\tF11"
            },
            "fullscreen",
            None,
            None,
        ),
    ]
}
unsafe fn append(
    menu: HMENU,
    items: Vec<Item>,
    actions: &mut Vec<WatchControl>,
) -> Result<(), AppError> {
    unsafe {
        for item in items {
            let title = item
                .title
                .replace('&', "&&")
                .encode_utf16()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let mut flags = MF_STRING;
            if item.checked {
                flags |= MF_CHECKED;
            }
            if item.disabled {
                flags |= MF_GRAYED;
            }
            let id = if let Some(control) = item.action {
                actions.push(control);
                actions.len()
            } else if !item.children.is_empty() {
                let sub = CreatePopupMenu()
                    .map_err(|e| AppError::new("player_unavailable", e.to_string()))?;
                if let Err(error) = append(sub, item.children, actions) {
                    let _ = DestroyMenu(sub);
                    return Err(error);
                }
                flags |= MF_POPUP;
                // AppendMenu owns this submenu after success.
                if let Err(error) = AppendMenuW(menu, flags, sub.0 as usize, PCWSTR(title.as_ptr()))
                {
                    let _ = DestroyMenu(sub);
                    return Err(AppError::new("player_unavailable", error.to_string()));
                }
                continue;
            } else {
                0
            };
            AppendMenuW(menu, flags, id, PCWSTR(title.as_ptr()))
                .map_err(|e| AppError::new("player_unavailable", e.to_string()))?;
        }
        Ok(())
    }
}
pub fn popup(
    host: isize,
    state: &PlayerSnapshot,
    fullscreen: bool,
) -> Result<Option<WatchControl>, AppError> {
    unsafe {
        let menu =
            CreatePopupMenu().map_err(|e| AppError::new("player_unavailable", e.to_string()))?;
        let result = (|| {
            let mut actions = vec![];
            append(menu, items(state, fullscreen), &mut actions)?;
            let mut point = POINT::default();
            GetCursorPos(&mut point)
                .map_err(|e| AppError::new("player_unavailable", e.to_string()))?;
            let id = TrackPopupMenuEx(
                menu,
                (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
                point.x,
                point.y,
                HWND(host as *mut _),
                None,
            )
            .0 as usize;
            Ok(if id > 0 {
                actions.into_iter().nth(id - 1)
            } else {
                None
            })
        })();
        let _ = DestroyMenu(menu);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{WatchChapter, WatchTrack};
    #[test]
    fn normal_native_menu() {
        let state = PlayerSnapshot {
            loaded: true,
            tracks: vec![
                WatchTrack {
                    id: 1,
                    kind: "audio".into(),
                    title: "English".into(),
                    selected: true,
                    secondary: false,
                },
                WatchTrack {
                    id: 1,
                    kind: "sub".into(),
                    title: "English".into(),
                    selected: true,
                    secondary: false,
                },
                WatchTrack {
                    id: 2,
                    kind: "sub".into(),
                    title: "中文".into(),
                    selected: false,
                    secondary: true,
                },
            ],
            chapters: vec![
                WatchChapter {
                    index: 0,
                    title: "第一段".into(),
                    position: 0.0,
                },
                WatchChapter {
                    index: 1,
                    title: "第二段".into(),
                    position: 4.0,
                },
            ],
            ..Default::default()
        };
        unsafe {
            let menu = CreatePopupMenu().unwrap();
            let mut actions = vec![];
            let result = append(menu, items(&state, true), &mut actions);
            let count = GetMenuItemCount(Some(menu));
            let _ = DestroyMenu(menu);
            result.unwrap();
            assert!(count > 0);
            for (action, value, text) in [
                ("secondary_subtitle", Some(2.0), None),
                ("chapter", Some(1.0), None),
                ("loop_mode", None, Some("playlist")),
                ("seek_prompt", None, None),
            ] {
                assert!(
                    actions.iter().any(|a| a.action == action
                        && a.value == value
                        && a.text.as_deref() == text)
                );
            }
        }
    }
}
