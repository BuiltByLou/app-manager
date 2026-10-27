mod i18n;
mod view_model;

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Alignment, Length, Size};
use cosmic::widget::{self, button, container, nav_bar, scrollable, text};
use cosmic::{Application, Element};

use appmgr_core::inventory::{App as InstalledApp, InstallKind, Origin, build_inventory};
use appmgr_core::real::RealProbe;
use appmgr_core::removal::{RemovalPlan, plan_for, removal_command, run_removal};
use view_model::{Group, GroupKind, apps_for, filter, format_size, system_groups};

const APP_ID: &str = "io.github.builtbylou.app_manager";

fn main() -> cosmic::iced::Result {
    let settings = Settings::default().size(Size::new(1000.0, 680.0));
    cosmic::app::run::<AppManager>(settings, ())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    User,
    System,
    Maintainer,
}

impl Page {
    const ALL: [Page; 3] = [Page::User, Page::System, Page::Maintainer];

    fn title(self) -> String {
        match self {
            Page::User => fl!("nav-user"),
            Page::System => fl!("nav-system"),
            Page::Maintainer => fl!("nav-maintainer"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Page::User => "system-users-symbolic",
            Page::System => "computer-symbolic",
            Page::Maintainer => "software-update-available-symbolic",
        }
    }
}

/// Where a removal is in its life cycle. The dialog follows this.
enum Stage {
    Planning,
    Ready(RemovalPlan),
    Running,
    Failed(String),
}

struct RemovalState {
    app: InstalledApp,
    stage: Stage,
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
    Loaded(Vec<InstalledApp>),
    Search(String),
    Select(String),
    CloseDetail,
    ToggleProtected,
    OpenMenu(String),
    CloseMenu,
    OpenDetails(String),
    StartRemoval(String),
    Planned(RemovalPlan),
    ConfirmRemoval,
    CancelRemoval,
    Removed(Result<(), String>),
}

pub struct AppManager {
    core: Core,
    nav_model: nav_bar::Model,
    apps: Vec<InstalledApp>,
    query: String,
    loading: bool,
    /// Id of the app whose details are open.
    selected: Option<String>,
    /// Whether locked system apps are listed.
    show_protected: bool,
    /// Id of the app whose three-dot menu is open.
    menu: Option<String>,
    /// The removal in progress, shown in the dialog.
    removal: Option<RemovalState>,
}

impl Application for AppManager {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        let mut nav_model = nav_bar::Model::default();
        for page in Page::ALL {
            nav_model
                .insert()
                .text(page.title())
                .icon(widget::icon::from_name(page.icon()))
                .data(page);
        }
        nav_model.activate_position(0);

        (
            Self {
                core,
                nav_model,
                apps: Vec::new(),
                query: String::new(),
                loading: true,
                selected: None,
                show_protected: true,
                menu: None,
                removal: None,
            },
            load_inventory(),
        )
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav_model)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Message> {
        self.nav_model.activate(id);
        self.selected = None;
        self.menu = None;
        Task::none()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Refresh => {
                self.loading = true;
                self.selected = None;
                self.menu = None;
                load_inventory()
            }
            Message::Loaded(apps) => {
                self.apps = apps;
                self.loading = false;
                if self.selected_app().is_none() {
                    self.selected = None;
                }
                Task::none()
            }
            Message::Search(query) => {
                self.query = query;
                self.menu = None;
                if let Some(selected) = self.selected.clone()
                    && !filter(&self.apps, &self.query)
                        .iter()
                        .any(|app| app.id == selected)
                {
                    self.selected = None;
                }
                Task::none()
            }
            Message::Select(id) => {
                self.menu = None;
                self.selected = Some(id);
                Task::none()
            }
            Message::CloseDetail => {
                self.selected = None;
                Task::none()
            }
            Message::ToggleProtected => {
                self.show_protected = !self.show_protected;
                if self
                    .selected_app()
                    .is_some_and(|app| app.protected && !self.show_protected)
                {
                    self.selected = None;
                }
                Task::none()
            }
            Message::OpenMenu(id) => {
                self.menu = Some(id);
                Task::none()
            }
            Message::CloseMenu => {
                self.menu = None;
                Task::none()
            }
            Message::OpenDetails(id) => {
                self.menu = None;
                self.selected = Some(id);
                Task::none()
            }
            Message::StartRemoval(id) => self.begin_removal(id),
            Message::Planned(plan) => {
                if let Some(state) = &mut self.removal
                    && matches!(state.stage, Stage::Planning)
                {
                    state.stage = Stage::Ready(plan);
                }
                Task::none()
            }
            Message::ConfirmRemoval => {
                let Some(state) = &self.removal else {
                    return Task::none();
                };
                if !matches!(state.stage, Stage::Ready(_)) {
                    return Task::none();
                }
                let app = state.app.clone();
                if let Some(state) = &mut self.removal {
                    state.stage = Stage::Running;
                }
                remove_app(app)
            }
            Message::CancelRemoval => {
                self.removal = None;
                Task::none()
            }
            Message::Removed(result) => match result {
                Ok(()) => {
                    self.removal = None;
                    self.selected = None;
                    self.menu = None;
                    self.loading = true;
                    load_inventory()
                }
                Err(error) => {
                    if let Some(state) = &mut self.removal {
                        state.stage = Stage::Failed(error);
                    }
                    Task::none()
                }
            },
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let page = self.page();
        let mut content = widget::column![text::title3(page.title())].spacing(12);

        if matches!(page, Page::User | Page::System) {
            content = content.push(self.search_row());
        }

        let body = self.body(page);
        if let Some(app) = self.selected_app() {
            content = content.push(
                widget::row![body, self.detail(app)]
                    .spacing(16)
                    .height(Length::Fill),
            );
        } else {
            content = content.push(body);
        }

        container(content.padding(20).width(Length::Fill).height(Length::Fill)).into()
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        self.removal_dialog()
    }
}

impl AppManager {
    fn page(&self) -> Page {
        self.nav_model
            .active_data::<Page>()
            .copied()
            .unwrap_or(Page::User)
    }

    fn selected_app(&self) -> Option<&InstalledApp> {
        let id = self.selected.as_deref()?;
        self.apps.iter().find(|app| app.id == id)
    }

    fn search_row(&self) -> Element<'_, Message> {
        let search = widget::text_input(fl!("search-placeholder"), &self.query)
            .on_input(Message::Search)
            .width(Length::Fill);

        let refresh = if self.loading {
            button::standard(fl!("refreshing"))
        } else {
            button::standard(fl!("refresh")).on_press(Message::Refresh)
        };

        widget::row![search, refresh]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
    }

    fn body(&self, page: Page) -> Element<'_, Message> {
        if self.loading && self.apps.is_empty() {
            return centred(fl!("loading-apps"));
        }

        match page {
            Page::Maintainer => centred(fl!("maintainer-soon")),
            Page::User => self.list_page(apps_for(&self.apps, InstallKind::User, &self.query)),
            Page::System => {
                self.system_page(system_groups(&self.apps, &self.query, self.show_protected))
            }
        }
    }

    fn list_page(&self, apps: Vec<InstalledApp>) -> Element<'_, Message> {
        if apps.is_empty() {
            return centred(self.empty_message());
        }

        let count = apps.len() as i64;
        let mut list = widget::column![].spacing(4);
        list = list.push(text::caption(fl!("apps-count", count = count)));
        for app in &apps {
            list = list.push(app_row(app, self.is_selected(&app.id), self.is_menu_open(&app.id)));
        }

        scrollable(list).height(Length::Fill).into()
    }

    fn system_page(&self, groups: Vec<Group>) -> Element<'_, Message> {
        let mut list = widget::column![].spacing(4);
        let protected = self.protected_count();

        if protected > 0 {
            let count = protected as i64;
            let label = if self.show_protected {
                fl!("hide-protected", count = count)
            } else {
                fl!("show-protected", count = count)
            };
            list = list.push(
                widget::row![
                    button::text(label).on_press(Message::ToggleProtected),
                    widget::Space::new().width(Length::Fill),
                ]
                .padding([0, 0, 8, 0]),
            );
        }

        if groups.is_empty() {
            if !self.query.trim().is_empty() {
                list = list.push(text::body(fl!("no-results")));
            } else if self.show_protected || protected == 0 {
                list = list.push(text::body(fl!("no-apps")));
            }
        }

        for group in groups {
            let count = group.apps.len() as i64;
            list = list.push(text::title3(group_title(group.kind)));
            list = list.push(text::caption(format!(
                "{} · {}",
                group_caption(group.kind),
                fl!("apps-count", count = count)
            )));
            for app in &group.apps {
                list = list.push(app_row(
                    app,
                    self.is_selected(&app.id),
                    self.is_menu_open(&app.id),
                ));
            }
        }

        scrollable(list).height(Length::Fill).into()
    }

    /// Locked system apps that match the current search, shown or not.
    fn protected_count(&self) -> usize {
        apps_for(&self.apps, InstallKind::System, &self.query)
            .iter()
            .filter(|app| app.protected)
            .count()
    }

    fn is_selected(&self, id: &str) -> bool {
        self.selected.as_deref() == Some(id)
    }

    fn is_menu_open(&self, id: &str) -> bool {
        self.menu.as_deref() == Some(id)
    }

    fn empty_message(&self) -> String {
        if self.query.trim().is_empty() {
            fl!("no-apps")
        } else {
            fl!("no-results")
        }
    }

    /// Starts a removal: asks the system what it would do, then shows the
    /// dialog. Nothing is removed until the user confirms.
    fn begin_removal(&mut self, id: String) -> Task<Message> {
        let Some(app) = self
            .apps
            .iter()
            .find(|app| app.id == id && !app.protected)
            .cloned()
        else {
            return Task::none();
        };

        self.menu = None;
        self.removal = Some(RemovalState {
            app: app.clone(),
            stage: Stage::Planning,
        });

        Task::perform(
            async move {
                let probe = RealProbe::new();
                plan_for(&app, |package| probe.simulate_apt_remove(package))
            },
            |plan| cosmic::Action::App(Message::Planned(plan)),
        )
    }

    fn removal_dialog(&self) -> Option<Element<'_, Message>> {
        let state = self.removal.as_ref()?;
        let app = &state.app;
        let mut dialog = widget::dialog::dialog()
            .title(fl!("remove-title"))
            .max_width(520.0);

        match &state.stage {
            Stage::Planning => {
                dialog = dialog.body(fl!("remove-planning")).secondary_action(
                    button::standard(fl!("remove-cancel")).on_press(Message::CancelRemoval),
                );
            }
            Stage::Running => {
                dialog = dialog
                    .body(fl!("remove-running"))
                    .control(text::caption(fl!("remove-running-note")))
                    .secondary_action(
                        button::standard(fl!("remove-running-close"))
                            .on_press(Message::CancelRemoval),
                    );
            }
            Stage::Failed(error) => {
                dialog = dialog
                    .body(fl!("remove-failed"))
                    .control(text::caption(error.clone()))
                    .primary_action(
                        button::standard(fl!("remove-close"))
                            .on_press(Message::CancelRemoval),
                    );
            }
            Stage::Ready(plan) => {
                let mut controls: Vec<Element<'_, Message>> = Vec::new();

                if let Some(error) = &plan.error {
                    dialog = dialog.body(fl!("remove-refused"));
                    controls.push(text::caption(error.clone()).into());
                } else {
                    dialog = dialog.body(fl!("remove-body", name = app.name.clone()));
                    controls.push(text::caption(fl!("remove-package")).into());
                    controls.push(text::body(plan.target.clone()).into());

                    if !plan.extra_packages.is_empty() {
                        controls.push(text::caption(fl!("remove-extra")).into());
                        for package in &plan.extra_packages {
                            controls.push(text::caption(package.clone()).into());
                        }
                    }

                    if !plan.kept_orphans.is_empty() {
                        let count = plan.kept_orphans.len() as i64;
                        controls.push(text::caption(fl!("remove-kept", count = count)).into());
                    }

                    match plan.origin {
                        Origin::Apt => {
                            controls.push(text::caption(fl!("remove-shared-libs")).into());
                        }
                        Origin::Flatpak => {
                            controls.push(text::caption(fl!("remove-flatpak-note")).into());
                        }
                        Origin::AppImage => {
                            controls.push(text::caption(fl!("remove-appimage-note")).into());
                        }
                    }
                }

                for control in controls {
                    dialog = dialog.control(control);
                }

                dialog = match removal_command(app) {
                    Ok(command) => {
                        if command.needs_permission() {
                            dialog.control(text::caption(fl!("remove-needs-permission")))
                        } else {
                            dialog
                        }
                        .primary_action(
                            button::destructive(fl!("remove-confirm"))
                                .on_press(Message::ConfirmRemoval),
                        )
                        .secondary_action(
                            button::standard(fl!("remove-cancel"))
                                .on_press(Message::CancelRemoval),
                        )
                    }
                    Err(error) => dialog
                        .control(text::caption(error))
                        .primary_action(
                            button::standard(fl!("remove-close"))
                                .on_press(Message::CancelRemoval),
                        ),
                };
            }
        }

        Some(dialog.into())
    }

    fn detail(&self, app: &InstalledApp) -> Element<'_, Message> {
        let mut content = widget::column![].spacing(12);
        content = content.push(self.pane_header(fl!("details")));

        let icon_name = app
            .icon
            .clone()
            .unwrap_or_else(|| "application-x-executable".to_owned());
        let icon: Element<'_, Message> = widget::icon::from_name(icon_name).size(64).into();
        content = content.push(icon);
        content = content.push(text::title2(app.name.clone()));
        let mut fields = widget::column![].spacing(10);
        fields = fields.push(field(fl!("detail-version"), version_label(app)));
        fields = fields.push(field(fl!("detail-origin"), origin_label(app.origin)));
        fields = fields.push(field(fl!("detail-size"), format_size(app.size_bytes)));
        if let Some(package) = &app.package {
            fields = fields.push(field(fl!("detail-package"), package.clone()));
        }
        fields = fields.push(field(
            fl!("detail-status"),
            if app.protected {
                fl!("status-protected")
            } else {
                fl!("status-removable")
            },
        ));
        content = content.push(fields);

        let remove = if app.protected {
            widget::column![
                button::destructive(fl!("remove")),
                text::caption(fl!("remove-blocked")),
            ]
            .spacing(6)
        } else {
            widget::column![
                button::destructive(fl!("remove")).on_press(Message::StartRemoval(app.id.clone()))
            ]
        };
        content = content.push(remove);

        pane(scrollable(content).height(Length::Fill).into())
    }

    fn pane_header(&self, title: String) -> Element<'_, Message> {
        widget::row![
            container(text::title3(title)).width(Length::Fill),
            button::icon(widget::icon::from_name("window-close-symbolic").size(16))
                .on_press(Message::CloseDetail),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }
}

fn load_inventory() -> Task<Message> {
    Task::perform(async { build_inventory(&RealProbe::new()) }, |apps| {
        cosmic::Action::App(Message::Loaded(apps))
    })
}

/// Runs the removal off the UI thread. `pkexec` and `gio` may wait for a
/// password or a trash operation, so the app must stay responsive.
fn remove_app(app: InstalledApp) -> Task<Message> {
    Task::perform(
        async move {
            let (sender, receiver) = cosmic::iced::futures::channel::oneshot::channel();
            std::thread::spawn(move || {
                let _ = sender.send(run_removal(&app));
            });
            receiver
                .await
                .unwrap_or_else(|_| Err("the removal stopped unexpectedly".to_owned()))
        },
        |result| cosmic::Action::App(Message::Removed(result)),
    )
}

fn centred(message: String) -> Element<'static, Message> {
    container(text::body(message))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

fn pane(content: Element<'_, Message>) -> Element<'_, Message> {
    container(content)
        .width(Length::Fixed(320.0))
        .height(Length::Fill)
        .padding(16)
        .class(cosmic::theme::Container::Card)
        .into()
}

fn field(label: String, value: String) -> Element<'static, Message> {
    widget::column![text::caption(label), text::body(value)]
        .spacing(2)
        .into()
}

fn group_title(kind: GroupKind) -> String {
    match kind {
        GroupKind::Protected => fl!("protected-title"),
        GroupKind::Removable => fl!("removable-title"),
    }
}

fn group_caption(kind: GroupKind) -> String {
    match kind {
        GroupKind::Protected => fl!("protected-caption"),
        GroupKind::Removable => fl!("removable-caption"),
    }
}

/// One app row with its three-dot menu. Clicking the row opens the details;
/// the menu opens on top when `menu_open`.
fn app_row(app: &InstalledApp, selected: bool, menu_open: bool) -> Element<'static, Message> {
    let icon_name = app
        .icon
        .clone()
        .unwrap_or_else(|| "application-x-executable".to_owned());
    let icon = widget::icon::from_name(icon_name).size(32);

    let title = text(app.name.clone()).size(15);
    let subtitle = text(format!(
        "{} · {} · {}",
        origin_label(app.origin),
        version_label(app),
        format_size(app.size_bytes),
    ))
    .size(12);

    let labels = widget::column![title, subtitle]
        .spacing(2)
        .width(Length::Fill);

    let mut row = widget::row![icon, labels]
        .spacing(12)
        .align_y(Alignment::Center);
    if app.protected {
        row = row.push(widget::icon::from_name("changes-prevent-symbolic").size(16));
    }

    let menu_button: Element<'static, Message> =
        button::icon(widget::icon::from_name("open-menu-symbolic").size(16))
            .on_press(Message::OpenMenu(app.id.clone()))
            .into();
    row = row.push(menu_button);

    let style = if selected {
        cosmic::theme::Container::custom(|theme| {
            let cosmic = theme.cosmic();
            let mut background: cosmic::iced::Color = cosmic.accent.base.into();
            background.a = 0.20;
            cosmic::iced::widget::container::Style {
                background: Some(cosmic::iced::Background::Color(background)),
                border: cosmic::iced::Border {
                    radius: cosmic.corner_radii.radius_s.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
    } else {
        cosmic::theme::Container::Transparent
    };

    let row = container(row)
        .padding(8)
        .width(Length::Fill)
        .class(style);

    let content: Element<'static, Message> = widget::mouse_area(row)
        .on_press(Message::Select(app.id.clone()))
        .into();

    if menu_open {
        widget::popover(content)
            .popup(menu_popup(app))
            .position(widget::popover::Position::Bottom)
            .on_close(Message::CloseMenu)
            .into()
    } else {
        content
    }
}

fn menu_popup(app: &InstalledApp) -> Element<'static, Message> {
    let details: Element<'static, Message> =
        button::text(fl!("details")).on_press(Message::OpenDetails(app.id.clone())).into();

    let mut menu = widget::column![details].spacing(4).width(Length::Fixed(220.0));
    if app.protected {
        menu = menu.push(text::caption(fl!("remove-blocked")));
    } else {
        let remove: Element<'static, Message> = button::destructive(fl!("remove"))
            .on_press(Message::StartRemoval(app.id.clone()))
            .into();
        menu = menu.push(remove);
    }

    container(menu)
        .padding(6)
        .class(cosmic::theme::Container::Dropdown)
        .into()
}

fn origin_label(origin: Origin) -> String {
    match origin {
        Origin::Apt => fl!("origin-apt"),
        Origin::Flatpak => fl!("origin-flatpak"),
        Origin::AppImage => fl!("origin-appimage"),
    }
}

fn version_label(app: &InstalledApp) -> String {
    app.version
        .clone()
        .unwrap_or_else(|| fl!("unknown-version"))
}
