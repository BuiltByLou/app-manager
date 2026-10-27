mod i18n;
mod prefs;
mod view_model;

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Alignment, Length, Size};
use cosmic::widget::{self, button, container, nav_bar, scrollable, text};
use cosmic::{Application, Element};

use appmgr_core::inventory::{App as InstalledApp, InstallKind, Origin, build_inventory};
use appmgr_core::real::RealProbe;
use appmgr_core::removal::{
    RemovalPlan, authorize, is_authorized, keep_sudo_alive, plan_for, removal_command, run_removal,
};
use prefs::Preferences;
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

/// The modal currently on screen, if any.
enum Dialog {
    /// Asks for the administrator password once for the whole session.
    Auth {
        pending: Option<InstalledApp>,
        checking: bool,
        error: Option<String>,
    },
    /// One-sentence confirmation from the three-dot menu.
    QuickConfirm {
        app: InstalledApp,
        dont_ask: bool,
    },
    /// Asks apt whether anything else would go, behind a quick removal.
    QuickCheck {
        app: InstalledApp,
    },
    /// Computes the full plan for a removal started from the details panel.
    Planning {
        app: InstalledApp,
    },
    /// Full plan with packages, dependents and kept libraries.
    Plan {
        app: InstalledApp,
        plan: RemovalPlan,
    },
    Running,
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
    Loaded(Vec<InstalledApp>),
    Search(String),
    OpenDetails(String),
    Select(String),
    CloseDetail,
    ToggleProtected,
    OpenMenu(String),
    CloseMenu,
    StartQuickRemoval(String),
    StartDetailedRemoval(String),
    ToggleDontAsk(bool),
    ConfirmQuick,
    CancelDialog,
    QuickChecked {
        id: String,
        plan: RemovalPlan,
    },
    PlanReady {
        id: String,
        plan: RemovalPlan,
    },
    ConfirmPlan,
    AuthChecked(bool),
    PasswordChanged(String),
    SubmitPassword,
    AuthResult(Result<(), String>),
    SkipAuth,
    Removed {
        app: InstalledApp,
        result: Result<(), String>,
    },
    SetConfirmRemovals(bool),
}

pub struct AppManager {
    core: Core,
    nav_model: nav_bar::Model,
    apps: Vec<InstalledApp>,
    query: String,
    loading: bool,
    /// Id of the app whose row is highlighted.
    selected: Option<String>,
    /// Id of the app whose details panel is open.
    detail: Option<String>,
    /// Whether locked system apps are listed.
    show_protected: bool,
    /// Id of the app whose three-dot menu is open.
    menu: Option<String>,
    dialog: Option<Dialog>,
    /// Sudo already holds an authorization for this session.
    authorized: bool,
    keepalive_started: bool,
    password: String,
    /// Persisted: ask before a quick removal.
    confirm_removals: bool,
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
                detail: None,
                show_protected: true,
                menu: None,
                dialog: None,
                authorized: false,
                keepalive_started: false,
                password: String::new(),
                confirm_removals: Preferences::load().confirm_removals,
            },
            Task::batch([load_inventory(), check_auth()]),
        )
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav_model)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Message> {
        self.nav_model.activate(id);
        self.selected = None;
        self.detail = None;
        self.menu = None;
        Task::none()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Refresh => {
                self.loading = true;
                self.selected = None;
                self.detail = None;
                self.menu = None;
                load_inventory()
            }
            Message::Loaded(apps) => {
                self.apps = apps;
                self.loading = false;
                if self.selected_app().is_none() {
                    self.selected = None;
                }
                if self.detail_app().is_none() {
                    self.detail = None;
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
            Message::OpenDetails(id) => {
                self.menu = None;
                self.selected = Some(id.clone());
                self.detail = Some(id);
                Task::none()
            }
            Message::Select(id) => {
                self.selected = Some(id);
                Task::none()
            }
            Message::CloseDetail => {
                self.detail = None;
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
            Message::StartQuickRemoval(id) => {
                self.menu = None;
                let Some(app) = self.removable_app(&id) else {
                    return Task::none();
                };
                if self.confirm_removals {
                    self.dialog = Some(Dialog::QuickConfirm { app, dont_ask: false });
                    Task::none()
                } else {
                    self.begin_quick_check(app)
                }
            }
            Message::StartDetailedRemoval(id) => {
                self.menu = None;
                let Some(app) = self.removable_app(&id) else {
                    return Task::none();
                };
                self.selected = Some(app.id.clone());
                self.dialog = Some(Dialog::Planning { app: app.clone() });
                detailed_plan(app)
            }
            Message::ToggleDontAsk(dont_ask) => {
                if let Some(Dialog::QuickConfirm {
                    dont_ask: current, ..
                }) = &mut self.dialog
                {
                    *current = dont_ask;
                }
                Task::none()
            }
            Message::ConfirmQuick => {
                let Some(Dialog::QuickConfirm { app, dont_ask }) = self.dialog.take() else {
                    return Task::none();
                };
                if dont_ask {
                    self.set_confirm_removals(false);
                }
                self.begin_quick_check(app)
            }
            Message::CancelDialog => {
                self.close_dialog();
                Task::none()
            }
            Message::QuickChecked { id, plan } => {
                let Some(Dialog::QuickCheck { app }) = self.dialog.take() else {
                    return Task::none();
                };
                if app.id != id {
                    self.dialog = Some(Dialog::QuickCheck { app });
                    return Task::none();
                }
                if plan.error.is_some() || !plan.extra_packages.is_empty() {
                    // Safer path: show what else would go before running it.
                    self.dialog = Some(Dialog::Plan { app, plan });
                    Task::none()
                } else {
                    self.execute(app)
                }
            }
            Message::PlanReady { id, plan } => {
                if let Some(Dialog::Planning { app }) = self.dialog.take() {
                    if app.id == id {
                        self.dialog = Some(Dialog::Plan { app, plan });
                    } else {
                        self.dialog = Some(Dialog::Planning { app });
                    }
                }
                Task::none()
            }
            Message::ConfirmPlan => {
                let Some(Dialog::Plan { app, .. }) = self.dialog.take() else {
                    return Task::none();
                };
                self.execute(app)
            }
            Message::AuthChecked(authorized) => {
                self.authorized = authorized;
                if authorized {
                    self.start_keepalive();
                    if matches!(
                        self.dialog,
                        Some(Dialog::Auth {
                            pending: None,
                            ..
                        })
                    ) {
                        self.dialog = None;
                    }
                } else if self.dialog.is_none() {
                    self.dialog = Some(Dialog::Auth {
                        pending: None,
                        checking: false,
                        error: None,
                    });
                }
                Task::none()
            }
            Message::PasswordChanged(password) => {
                self.password = password;
                if let Some(Dialog::Auth { error, .. }) = &mut self.dialog {
                    *error = None;
                }
                Task::none()
            }
            Message::SubmitPassword => {
                if let Some(Dialog::Auth { checking, error, .. }) = &mut self.dialog {
                    *checking = true;
                    *error = None;
                } else {
                    return Task::none();
                }
                let password = std::mem::take(&mut self.password);
                Task::perform(
                    async move {
                        let result = authorize(&password);
                        wipe(password);
                        result
                    },
                    |result| cosmic::Action::App(Message::AuthResult(result)),
                )
            }
            Message::AuthResult(result) => match result {
                Ok(()) => {
                    self.authorized = true;
                    self.start_keepalive();
                    let pending = match self.dialog.take() {
                        Some(Dialog::Auth { pending, .. }) => pending,
                        other => {
                            self.dialog = other;
                            None
                        }
                    };
                    if let Some(app) = pending {
                        self.execute(app)
                    } else {
                        Task::none()
                    }
                }
                Err(error) => {
                    eprintln!("appmgr: administrator authorization failed: {error}");
                    if let Some(Dialog::Auth {
                        checking, error: shown,
                        ..
                    }) = &mut self.dialog
                    {
                        *checking = false;
                        *shown = Some(fl!("auth-failed"));
                    }
                    Task::none()
                }
            },
            Message::SkipAuth => {
                self.close_dialog();
                Task::none()
            }
            Message::Removed { app, result } => match result {
                Ok(()) => {
                    if matches!(self.dialog, Some(Dialog::Running)) {
                        self.dialog = None;
                    }
                    if self.selected.as_deref() == Some(app.id.as_str()) {
                        self.selected = None;
                    }
                    if self.detail.as_deref() == Some(app.id.as_str()) {
                        self.detail = None;
                    }
                    self.loading = true;
                    load_inventory()
                }
                Err(error) => {
                    if needs_password(&error) {
                        self.authorized = false;
                        if matches!(self.dialog, Some(Dialog::Running) | None) {
                            self.dialog = Some(Dialog::Auth {
                                pending: Some(app),
                                checking: false,
                                error: None,
                            });
                        }
                    } else if matches!(self.dialog, Some(Dialog::Running)) {
                        self.dialog = Some(Dialog::Failed { error });
                    }
                    Task::none()
                }
            },
            Message::SetConfirmRemovals(confirm) => {
                self.set_confirm_removals(confirm);
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let page = self.page();
        let mut content = widget::column![text::title3(page.title())].spacing(12);

        if matches!(page, Page::User | Page::System) {
            content = content.push(self.search_row());
        }

        let body = self.body(page);
        if let Some(app) = self.detail_app() {
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

    fn detail_app(&self) -> Option<&InstalledApp> {
        let id = self.detail.as_deref()?;
        self.apps.iter().find(|app| app.id == id)
    }

    fn removable_app(&self, id: &str) -> Option<InstalledApp> {
        self.apps
            .iter()
            .find(|app| app.id == id && !app.protected)
            .cloned()
    }

    fn set_confirm_removals(&mut self, confirm: bool) {
        self.confirm_removals = confirm;
        Preferences {
            confirm_removals: confirm,
        }
        .save();
    }

    fn close_dialog(&mut self) {
        if matches!(self.dialog, Some(Dialog::Auth { .. })) {
            wipe(std::mem::take(&mut self.password));
        }
        self.dialog = None;
    }

    fn start_keepalive(&mut self) {
        if !self.keepalive_started {
            self.keepalive_started = true;
            keep_sudo_alive();
        }
    }

    /// Checks apt for a quick removal. If it would take other packages with
    /// it, the user sees the full plan before anything runs.
    fn begin_quick_check(&mut self, app: InstalledApp) -> Task<Message> {
        self.dialog = Some(Dialog::QuickCheck { app: app.clone() });
        quick_check(app)
    }

    /// Runs the removal, asking for the administrator password first if it
    /// is needed and not authorized yet.
    fn execute(&mut self, app: InstalledApp) -> Task<Message> {
        match removal_command(&app) {
            Err(error) => {
                self.dialog = Some(Dialog::Failed { error });
                Task::none()
            }
            Ok(command) if command.needs_permission() && !self.authorized => {
                self.dialog = Some(Dialog::Auth {
                    pending: Some(app),
                    checking: false,
                    error: None,
                });
                Task::none()
            }
            Ok(_) => {
                self.dialog = Some(Dialog::Running);
                remove_app(app)
            }
        }
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

    fn removal_dialog(&self) -> Option<Element<'_, Message>> {
        let dialog = self.dialog.as_ref()?;
        let mut window = widget::dialog::dialog()
            .title(fl!("remove-title"))
            .max_width(520.0)
            .max_height(480.0);

        match dialog {
            Dialog::Auth {
                pending,
                checking,
                error,
            } => {
                let body = if pending.is_some() {
                    fl!("auth-body-remove")
                } else {
                    fl!("auth-body")
                };
                let input: Element<'_, Message> =
                    widget::secure_input(fl!("auth-placeholder"), &self.password, None, true)
                        .on_input(Message::PasswordChanged)
                        .on_submit(|_| Message::SubmitPassword)
                        .into();
                window = window
                    .title(fl!("auth-title"))
                    .body(body)
                    .control(input);
                if let Some(error) = error {
                    window = window.control(text::caption(error.clone()));
                }
                if *checking {
                    window = window.control(text::caption(fl!("auth-checking")));
                } else {
                    window = window
                        .primary_action(
                            button::suggested(fl!("auth-continue"))
                                .on_press(Message::SubmitPassword),
                        )
                        .secondary_action(
                            button::standard(fl!("auth-skip")).on_press(Message::SkipAuth),
                        );
                }
            }
            Dialog::QuickConfirm { app, dont_ask } => {
                let dont_ask: Element<'_, Message> =
                    widget::checkbox(*dont_ask)
                        .label(fl!("remove-dont-ask"))
                        .on_toggle(Message::ToggleDontAsk)
                        .into();
                window = window
                    .body(fl!("remove-quick-body", name = app.name.clone()))
                    .control(dont_ask)
                    .primary_action(
                        button::destructive(fl!("remove-confirm"))
                            .on_press(Message::ConfirmQuick),
                    )
                    .secondary_action(
                        button::standard(fl!("remove-cancel"))
                            .on_press(Message::CancelDialog),
                    );
            }
            Dialog::QuickCheck { .. } => {
                window = window.body(fl!("remove-checking")).secondary_action(
                    button::standard(fl!("remove-cancel")).on_press(Message::CancelDialog),
                );
            }
            Dialog::Planning { .. } => {
                window = window.body(fl!("remove-planning")).secondary_action(
                    button::standard(fl!("remove-cancel")).on_press(Message::CancelDialog),
                );
            }
            Dialog::Plan { app, plan } => {
                window = plan_dialog(window, app, plan);
            }
            Dialog::Running => {
                window = window
                    .body(fl!("remove-running"))
                    .control(text::caption(fl!("remove-running-note")))
                    .secondary_action(
                        button::standard(fl!("remove-close")).on_press(Message::CancelDialog),
                    );
            }
            Dialog::Failed { error } => {
                window = window
                    .body(fl!("remove-failed"))
                    .control(text::caption(error.clone()))
                    .primary_action(
                        button::standard(fl!("remove-close")).on_press(Message::CancelDialog),
                    );
            }
        }

        Some(window.into())
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
                button::destructive(fl!("remove"))
                    .on_press(Message::StartDetailedRemoval(app.id.clone()))
            ]
        };
        content = content.push(remove);
        content = content.push(
            widget::checkbox(self.confirm_removals)
                .label(fl!("ask-before-removing"))
                .on_toggle(Message::SetConfirmRemovals),
        );

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

fn plan_dialog<'a>(
    window: widget::dialog::Dialog<'a, Message>,
    app: &InstalledApp,
    plan: &RemovalPlan,
) -> widget::dialog::Dialog<'a, Message> {
    if let Some(error) = &plan.error {
        return window
            .body(fl!("remove-refused"))
            .control(text::caption(error.clone()))
            .primary_action(button::standard(fl!("remove-close")).on_press(Message::CancelDialog));
    }

    let mut details = widget::column![text::caption(fl!("remove-package")), text::body(plan.target.clone())]
        .spacing(4);
    if !plan.extra_packages.is_empty() {
        details = details.push(text::caption(fl!("remove-extra")));
        for package in &plan.extra_packages {
            details = details.push(text::caption(package.clone()));
        }
    }
    if !plan.kept_orphans.is_empty() {
        let count = plan.kept_orphans.len() as i64;
        details = details.push(text::caption(fl!("remove-kept", count = count)));
    }
    match plan.origin {
        Origin::Apt => {
            details = details.push(text::caption(fl!("remove-shared-libs")));
        }
        Origin::Flatpak => {
            details = details.push(text::caption(fl!("remove-flatpak-note")));
        }
        Origin::AppImage => {
            details = details.push(text::caption(fl!("remove-appimage-note")));
        }
    }

    window
        .body(fl!("remove-body", name = app.name.clone()))
        .control(scrollable(details).height(Length::Fixed(220.0)))
        .primary_action(button::destructive(fl!("remove-confirm")).on_press(Message::ConfirmPlan))
        .secondary_action(button::standard(fl!("remove-cancel")).on_press(Message::CancelDialog))
}

fn load_inventory() -> Task<Message> {
    Task::perform(async { build_inventory(&RealProbe::new()) }, |apps| {
        cosmic::Action::App(Message::Loaded(apps))
    })
}

fn check_auth() -> Task<Message> {
    Task::perform(async { is_authorized() }, |authorized| {
        cosmic::Action::App(Message::AuthChecked(authorized))
    })
}

fn quick_check(app: InstalledApp) -> Task<Message> {
    let id = app.id.clone();
    Task::perform(
        async move {
            let probe = RealProbe::new();
            plan_for(&app, |package| probe.simulate_apt_remove(package))
        },
        move |plan| cosmic::Action::App(Message::QuickChecked { id, plan }),
    )
}

fn detailed_plan(app: InstalledApp) -> Task<Message> {
    let id = app.id.clone();
    Task::perform(
        async move {
            let probe = RealProbe::new();
            plan_for(&app, |package| probe.simulate_apt_remove(package))
        },
        move |plan| cosmic::Action::App(Message::PlanReady { id, plan }),
    )
}

/// Runs the removal off the UI thread. `sudo` and `gio` may wait on disk, so
/// the app must stay responsive.
fn remove_app(app: InstalledApp) -> Task<Message> {
    Task::perform(
        async move {
            let (sender, receiver) = cosmic::iced::futures::channel::oneshot::channel();
            let thread_app = app.clone();
            std::thread::spawn(move || {
                let _ = sender.send(run_removal(&thread_app));
            });
            let result = receiver
                .await
                .unwrap_or_else(|_| Err("the removal stopped unexpectedly".to_owned()));
            (app, result)
        },
        |(app, result)| cosmic::Action::App(Message::Removed { app, result }),
    )
}

/// sudo says this when its timestamp is gone, so the app can ask again.
fn needs_password(error: &str) -> bool {
    error.to_lowercase().contains("password is required")
}

/// Overwrites the password buffer before releasing it.
fn wipe(mut text: String) {
    unsafe {
        text.as_mut_vec().fill(0);
    }
    text.clear();
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

/// One app row with its three-dot menu. The row itself is not clickable:
/// details open from the menu, so removing stays a deliberate action.
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

    let menu_icon = button::icon(widget::icon::from_name("open-menu-symbolic").size(16))
        .on_press(Message::OpenMenu(app.id.clone()));
    let menu_button: Element<'static, Message> = if menu_open {
        widget::popover(menu_icon)
            .popup(menu_popup(app))
            .position(widget::popover::Position::Bottom)
            .on_close(Message::CloseMenu)
            .into()
    } else {
        menu_icon.into()
    };
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

    let row: Element<'static, Message> = container(row)
        .padding(8)
        .width(Length::Fill)
        .class(style)
        .into();

    widget::mouse_area(row)
        .on_press(Message::Select(app.id.clone()))
        .into()
}

fn menu_popup(app: &InstalledApp) -> Element<'static, Message> {
    let details: Element<'static, Message> = button::text(fl!("details"))
        .on_press(Message::OpenDetails(app.id.clone()))
        .into();

    let mut menu = widget::column![details].spacing(4).width(Length::Fixed(220.0));
    if app.protected {
        menu = menu.push(text::caption(fl!("remove-blocked")));
    } else {
        let remove: Element<'static, Message> = button::destructive(fl!("remove"))
            .on_press(Message::StartQuickRemoval(app.id.clone()))
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
