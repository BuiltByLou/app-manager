mod i18n;
mod view_model;

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Alignment, Length, Size};
use cosmic::widget::{self, button, container, nav_bar, scrollable, text};
use cosmic::{Application, Element};

use appmgr_core::inventory::{App as InstalledApp, InstallKind, Origin, build_inventory};
use appmgr_core::real::RealProbe;
use appmgr_core::removal::{RemovalPlan, plan_for};
use view_model::{Group, GroupKind, apps_for, format_size, system_groups};

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

    fn kind(self) -> Option<InstallKind> {
        match self {
            Page::User => Some(InstallKind::User),
            Page::System => Some(InstallKind::System),
            Page::Maintainer => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
    Loaded(Vec<InstalledApp>),
    Search(String),
    Select(String),
    CloseDetail,
    PreviewRemoval,
    Planned(RemovalPlan),
    BackToDetail,
}

pub struct AppManager {
    core: Core,
    nav_model: nav_bar::Model,
    apps: Vec<InstalledApp>,
    query: String,
    loading: bool,
    /// Id of the app whose details are open.
    selected: Option<String>,
    /// Preview shown instead of the details once computed.
    plan: Option<RemovalPlan>,
    planning: bool,
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
                plan: None,
                planning: false,
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
        self.plan = None;
        Task::none()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Refresh => {
                self.loading = true;
                self.selected = None;
                self.plan = None;
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
                Task::none()
            }
            Message::Select(id) => {
                self.plan = None;
                self.planning = false;
                self.selected = Some(id);
                Task::none()
            }
            Message::CloseDetail => {
                self.selected = None;
                self.plan = None;
                self.planning = false;
                Task::none()
            }
            Message::PreviewRemoval => {
                let Some(app) = self.selected_app().cloned() else {
                    return Task::none();
                };
                if app.protected {
                    return Task::none();
                }
                self.planning = true;
                self.plan = None;
                Task::perform(
                    async move {
                        let probe = RealProbe::new();
                        plan_for(&app, |package| probe.simulate_apt_remove(package))
                    },
                    |plan| cosmic::Action::App(Message::Planned(plan)),
                )
            }
            Message::Planned(plan) => {
                self.planning = false;
                self.plan = Some(plan);
                Task::none()
            }
            Message::BackToDetail => {
                self.plan = None;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let page = self.page();
        let mut content = widget::column![text::title3(page.title())].spacing(12);

        if page.kind().is_some() {
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
            Page::System => self.system_page(system_groups(&self.apps, &self.query)),
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
            list = list.push(app_row(app, self.is_selected(&app.id)));
        }

        scrollable(list).height(Length::Fill).into()
    }

    fn system_page(&self, groups: Vec<Group>) -> Element<'_, Message> {
        if groups.is_empty() {
            return centred(self.empty_message());
        }

        let mut list = widget::column![].spacing(4);
        for group in groups {
            let count = group.apps.len() as i64;
            list = list.push(text::title3(group_title(group.kind)));
            list = list.push(text::caption(format!(
                "{} · {}",
                group_caption(group.kind),
                fl!("apps-count", count = count)
            )));
            for app in &group.apps {
                list = list.push(app_row(app, self.is_selected(&app.id)));
            }
        }

        scrollable(list).height(Length::Fill).into()
    }

    fn is_selected(&self, id: &str) -> bool {
        self.selected.as_deref() == Some(id)
    }

    fn empty_message(&self) -> String {
        if self.query.trim().is_empty() {
            fl!("no-apps")
        } else {
            fl!("no-results")
        }
    }

    fn detail(&self, app: &InstalledApp) -> Element<'_, Message> {
        if self.planning {
            return pane(
                widget::column![
                    self.pane_header(fl!("preview-title"), None),
                    text::body(fl!("planning")),
                ]
                .spacing(12)
                .into(),
            );
        }

        if let Some(plan) = &self.plan {
            return self.preview(app, plan);
        }

        self.app_details(app)
    }

    fn app_details(&self, app: &InstalledApp) -> Element<'_, Message> {
        let mut content = widget::column![].spacing(12);
        content = content.push(self.pane_header(fl!("details"), None));

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
                button::destructive(fl!("remove")).on_press(Message::PreviewRemoval)
            ]
        };
        content = content.push(remove);

        pane(scrollable(content).height(Length::Fill).into())
    }

    fn preview(&self, app: &InstalledApp, plan: &RemovalPlan) -> Element<'_, Message> {
        let mut content = widget::column![].spacing(12);
        content = content.push(self.pane_header(fl!("preview-title"), Some(Message::BackToDetail)));
        content = content.push(text::title2(app.name.clone()));
        content = content.push(text::caption(fl!("preview-simulation")));

        if let Some(error) = &plan.error {
            content = content.push(text::body(fl!("preview-error")));
            content = content.push(text::caption(error.clone()));
            return pane(scrollable(content).height(Length::Fill).into());
        }

        content = content.push(text::body(fl!("preview-will-remove")));
        let small_icon: Element<'_, Message> = widget::icon::from_name(
            app.icon
                .clone()
                .unwrap_or_else(|| "application-x-executable".to_owned()),
        )
        .size(20)
        .into();
        content = content.push(
            widget::row![small_icon, text::body(app.name.clone())]
                .spacing(8)
                .align_y(Alignment::Center),
        );

        if !plan.extra_packages.is_empty() {
            content = content.push(text::caption(fl!("preview-extra")));
            for package in &plan.extra_packages {
                content = content.push(text::caption(package.clone()));
            }
        }

        if !plan.kept_orphans.is_empty() {
            let count = plan.kept_orphans.len() as i64;
            content = content.push(text::caption(fl!("preview-kept", count = count)));
        }

        match plan.origin {
            Origin::Apt => {
                content = content.push(text::caption(fl!("preview-shared-libs")));
            }
            Origin::Flatpak => {
                content = content.push(text::caption(fl!("preview-flatpak-note")));
            }
            Origin::AppImage => {
                content = content.push(text::caption(fl!("preview-appimage-note")));
                content = content.push(text::caption(plan.target.clone()));
            }
        }

        content = content.push(button::standard(fl!("back")).on_press(Message::BackToDetail));

        pane(scrollable(content).height(Length::Fill).into())
    }

    fn pane_header(&self, title: String, back: Option<Message>) -> Element<'_, Message> {
        let mut row = widget::row![].spacing(8).align_y(Alignment::Center);
        if let Some(message) = back {
            row = row.push(
                button::icon(widget::icon::from_name("go-previous-symbolic").size(16))
                    .on_press(message),
            );
        }
        row = row.push(container(text::title3(title)).width(Length::Fill));
        row = row.push(
            button::icon(widget::icon::from_name("window-close-symbolic").size(16))
                .on_press(Message::CloseDetail),
        );
        row.into()
    }
}

fn load_inventory() -> Task<Message> {
    Task::perform(async { build_inventory(&RealProbe::new()) }, |apps| {
        cosmic::Action::App(Message::Loaded(apps))
    })
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

fn app_row(app: &InstalledApp, selected: bool) -> Element<'static, Message> {
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

    let mut row = widget::row![icon, labels].spacing(12).align_y(Alignment::Center);
    if app.protected {
        row = row.push(widget::icon::from_name("changes-prevent-symbolic").size(16));
    }

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

    widget::mouse_area(row)
        .on_press(Message::Select(app.id.clone()))
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
