mod view_model;

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Alignment, Length, Size};
use cosmic::widget::{self, button, container, nav_bar, scrollable, text};
use cosmic::{Application, Element};

use appmgr_core::inventory::{App as InstalledApp, InstallKind, Origin, build_inventory};
use appmgr_core::real::RealProbe;
use view_model::{format_size, apps_for};

const APP_ID: &str = "io.github.builtbylou.app_manager";

fn main() -> cosmic::iced::Result {
    let settings = Settings::default().size(Size::new(940.0, 660.0));
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

    fn title(self) -> &'static str {
        match self {
            Page::User => view_model::USER_TITLE,
            Page::System => view_model::SYSTEM_TITLE,
            Page::Maintainer => view_model::MAINTAINER_TITLE,
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
}

pub struct AppManager {
    core: Core,
    nav_model: nav_bar::Model,
    apps: Vec<InstalledApp>,
    query: String,
    loading: bool,
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
            },
            load_inventory(),
        )
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav_model)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Message> {
        self.nav_model.activate(id);
        Task::none()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Refresh => {
                self.loading = true;
                load_inventory()
            }
            Message::Loaded(apps) => {
                self.apps = apps;
                self.loading = false;
                Task::none()
            }
            Message::Search(query) => {
                self.query = query;
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
        content = content.push(self.body(page));

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

    fn search_row(&self) -> Element<'_, Message> {
        let search = widget::text_input("Pesquisar apps", &self.query)
            .on_input(Message::Search)
            .width(Length::Fill);

        let refresh = if self.loading {
            button::standard("A ler…")
        } else {
            button::standard("Atualizar").on_press(Message::Refresh)
        };

        widget::row![search, refresh]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
    }

    fn body(&self, page: Page) -> Element<'_, Message> {
        if self.loading && self.apps.is_empty() {
            return centred("A ler as apps instaladas…");
        }

        let Some(kind) = page.kind() else {
            return centred(
                "Instalar e atualizar apps a partir de um repositório GitHub \
                 é a próxima fase. Por agora, usa as apps instaladas.",
            );
        };

        let apps = apps_for(&self.apps, kind, &self.query);
        if apps.is_empty() {
            let message = if self.query.trim().is_empty() {
                "Nenhuma app neste grupo."
            } else {
                "Nada corresponde à pesquisa."
            };
            return centred(message);
        }

        let mut list = widget::column![].spacing(4);
        list = list.push(text::caption(format!("{} apps", apps.len())));
        for app in &apps {
            list = list.push(app_row(app));
        }

        scrollable(list).height(Length::Fill).into()
    }
}

fn load_inventory() -> Task<Message> {
    Task::perform(async { build_inventory(&RealProbe::new()) }, |apps| {
        cosmic::Action::App(Message::Loaded(apps))
    })
}

fn centred(message: &str) -> Element<'static, Message> {
    container(text(message.to_owned()))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

fn app_row(app: &InstalledApp) -> Element<'static, Message> {
    let icon_name = app
        .icon
        .clone()
        .unwrap_or_else(|| "application-x-executable".to_owned());
    let icon: Element<'static, Message> = widget::icon::from_name(icon_name).size(32).into();

    let title = text(app.name.clone()).size(15);
    let subtitle = text(format!(
        "{} · {} · {}",
        origin_label(app.origin),
        app.version.as_deref().unwrap_or("versão desconhecida"),
        format_size(app.size_bytes),
    ))
    .size(12);

    let labels = widget::column![title, subtitle]
        .spacing(2)
        .width(Length::Fill);

    container(widget::row![icon, labels].spacing(12).align_y(Alignment::Center))
        .padding(8)
        .width(Length::Fill)
        .into()
}

fn origin_label(origin: Origin) -> &'static str {
    match origin {
        Origin::Apt => "APT",
        Origin::Flatpak => "Flatpak",
        Origin::AppImage => "AppImage",
    }
}
