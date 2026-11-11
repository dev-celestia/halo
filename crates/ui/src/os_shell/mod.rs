//! Desktop shell: wallpaper icons, session widgets, and a bottom start bar.

mod app_window;
mod desktop;
mod dock;
mod helpers;
mod launcher;
mod notifications;
mod onboarding;
mod widget_cards;
mod widgets;
mod window_mockup;
mod work_tabs;

pub(crate) use app_window::render_app_window;
pub(crate) use desktop::render_desktop;
pub(crate) use dock::render_dock;
pub(crate) use launcher::render_launcher;
pub(crate) use notifications::render_notifications;
pub(crate) use onboarding::render_onboarding_dialog;
pub(crate) use widgets::DesktopWidget;
pub(crate) use window_mockup::{Mockup, render_mockup};
pub(crate) use work_tabs::render_work_tabs;
