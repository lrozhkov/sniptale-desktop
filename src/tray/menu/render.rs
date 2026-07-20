//! Runtime menu command id plan for native tray renderers.

use std::{error::Error, fmt};

use super::{
    actions::TrayActionKind,
    model::{TrayMenu, TrayMenuEntry, TrayMenuItem},
};

const FIRST_COMMAND_ID: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TrayMenuRenderError {
    CommandIdExhausted,
}

impl fmt::Display for TrayMenuRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandIdExhausted => formatter.write_str("tray menu command id exhausted"),
        }
    }
}

impl Error for TrayMenuRenderError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayMenuRenderItem {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) command_id: Option<u32>,
    pub(crate) enabled: bool,
    pub(crate) warning: Option<String>,
    pub(crate) shortcut_label: Option<String>,
    pub(crate) checked: bool,
    pub(crate) radio: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayMenuCommand {
    pub(crate) command_id: u32,
    pub(crate) action_id: String,
    pub(crate) action_kind: TrayActionKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayMenuRenderPlan {
    pub(crate) entries: Vec<TrayMenuRenderEntry>,
    pub(crate) commands: Vec<TrayMenuCommand>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TrayMenuRenderEntry {
    Item(TrayMenuRenderItem),
    Separator,
    Submenu {
        id: &'static str,
        label: &'static str,
        enabled: bool,
        entries: Vec<TrayMenuRenderEntry>,
    },
}

pub(crate) fn plan_menu_render(menu: &TrayMenu) -> Result<TrayMenuRenderPlan, TrayMenuRenderError> {
    let mut next_command_id = FIRST_COMMAND_ID;
    let mut commands = Vec::new();
    let entries = plan_entries(menu.entries(), &mut next_command_id, &mut commands)?;
    Ok(TrayMenuRenderPlan { entries, commands })
}

fn plan_entries(
    entries: &[TrayMenuEntry],
    next_command_id: &mut u32,
    commands: &mut Vec<TrayMenuCommand>,
) -> Result<Vec<TrayMenuRenderEntry>, TrayMenuRenderError> {
    let mut render_entries = Vec::with_capacity(entries.len());
    for entry in entries {
        match entry {
            TrayMenuEntry::Item(item) => {
                render_entries.push(TrayMenuRenderEntry::Item(plan_item(
                    item,
                    next_command_id,
                    commands,
                )?));
            }
            TrayMenuEntry::Separator => render_entries.push(TrayMenuRenderEntry::Separator),
            TrayMenuEntry::Submenu {
                id,
                label,
                enabled,
                entries,
            } => render_entries.push(TrayMenuRenderEntry::Submenu {
                id,
                label,
                enabled: *enabled,
                entries: plan_entries(entries, next_command_id, commands)?,
            }),
        }
    }
    Ok(render_entries)
}

fn plan_item(
    item: &TrayMenuItem,
    next_command_id: &mut u32,
    commands: &mut Vec<TrayMenuCommand>,
) -> Result<TrayMenuRenderItem, TrayMenuRenderError> {
    let command_id = if item.enabled() {
        item.action_kind()
            .map(|action_kind| {
                let command_id = *next_command_id;
                *next_command_id = next_command_id
                    .checked_add(1)
                    .ok_or(TrayMenuRenderError::CommandIdExhausted)?;
                commands.push(TrayMenuCommand {
                    command_id,
                    action_id: item.id().to_owned(),
                    action_kind,
                });
                Ok(command_id)
            })
            .transpose()?
    } else {
        None
    };
    Ok(TrayMenuRenderItem {
        id: item.id().to_owned(),
        label: item.label().to_owned(),
        command_id,
        enabled: item.enabled(),
        warning: item.warning().map(str::to_owned),
        shortcut_label: item.shortcut_label().map(str::to_owned),
        checked: item.checked(),
        radio: item.radio(),
    })
}
