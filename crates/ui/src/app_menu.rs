//! Native app menu bar — rebuilt when the focused page is Coder.

use crate::actions::{
    CloseCoderProject, NewCoderProject, OpenCoderFolder, OpenCoderRecent, OpenCoderSettings,
    SaveCoderFile, ToggleCoderQuickOpen, ToggleCoderWordWrap,
};
use crate::nav::Page;
use crate::pages::coder::project_display_name;
use celestia_ui::components::composite::window::{
    AppMenu, CloseWindow, MinimizeWindow, NewWindow, ToggleFullScreen, ZoomWindow,
};
use gpui::{App, Menu, MenuItem};

/// Install (or replace) the process menu bar for the focused `page`.
///
/// When `page` is [`Page::Coder`], File gains New Project / Open Folder /
/// Open Recent / Close Project / Save / Settings, and View gains Toggle Word Wrap.
/// Otherwise File stays New Window + Close.
pub fn install(page: Page, recent_projects: &[String], has_open_project: bool, cx: &App) {
    let file = if page == Page::Coder {
        coder_file_menu(recent_projects, has_open_project)
    } else {
        Menu::new("File").items(vec![
            MenuItem::action("New Window", NewWindow),
            MenuItem::action("Close", CloseWindow),
        ])
    };

    let mut menu = AppMenu::standard("Halo")
        .menu(file)
        .menu(Menu::new("Window").items([
            MenuItem::action("Minimize", MinimizeWindow),
            MenuItem::action("Zoom", ZoomWindow),
        ]));

    if page == Page::Coder {
        menu = menu
            .menu(Menu::new("Go").items([MenuItem::action("Go to File…", ToggleCoderQuickOpen)]));
        menu = menu.menu(Menu::new("View").items([
            MenuItem::action("Toggle Word Wrap", ToggleCoderWordWrap),
            MenuItem::separator(),
            MenuItem::action("Enter Full Screen", ToggleFullScreen),
        ]));
    }

    menu.install(cx);
}

fn coder_file_menu(recent_projects: &[String], has_open_project: bool) -> Menu {
    let mut items = vec![
        MenuItem::action("New Window", NewWindow),
        MenuItem::separator(),
        MenuItem::action("New Project…", NewCoderProject),
        MenuItem::action("Open Folder…", OpenCoderFolder),
    ];

    if recent_projects.is_empty() {
        items.push(
            MenuItem::submenu(Menu::new("Open Recent").items(vec![MenuItem::action(
                "No Recent Projects",
                OpenCoderRecent {
                    path: String::new(),
                },
            )]))
            .disabled(true),
        );
    } else {
        let recent_items: Vec<MenuItem> = recent_projects
            .iter()
            .map(|path| {
                MenuItem::action(
                    project_display_name(path),
                    OpenCoderRecent { path: path.clone() },
                )
            })
            .collect();
        items.push(MenuItem::submenu(
            Menu::new("Open Recent").items(recent_items),
        ));
    }

    items.push(MenuItem::action("Close Project", CloseCoderProject).disabled(!has_open_project));
    items.push(MenuItem::separator());
    items.push(MenuItem::action("Save", SaveCoderFile));
    items.push(MenuItem::separator());
    items.push(MenuItem::action("Settings…", OpenCoderSettings));
    items.push(MenuItem::separator());
    items.push(MenuItem::action("Close", CloseWindow));

    Menu::new("File").items(items)
}
