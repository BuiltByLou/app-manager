mod view_model;

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Alignment, Length, Size};
use cosmic::widget::{self, button, container, scrollable, text};
use cosmic::{Application, Element};

use appmgr_core::inventory::{App as InstalledApp, Origin, build_inventory};
use appmgr_core::real::RealProbe;
use view_model::{format_size, sections};

const APP_ID: &str = "io.github.builtbylou.app_manager";

fn main() -> cosmic::iced::Result {
    let settings = Settings::default().size(Size::new(880.0, 640.0));
    cosmic::app::run::<AppManager>(settings, ())
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
    Loaded(Vec<InstalledApp>),
    Search(String),
}

pub struct AppManager {
    core: Core,
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
        (
            Self {
                core,
                apps: Vec::new(),
                query: String::new(),
                loading: true,
            },
            Task::perform(async { build_inventory(&RealProbe::new()) }, |apps| {
                cosmic::Action::App(Message::Loaded(apps))
            }),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Refresh => {
                self.loading = true;
                Task::perform(async { build_inventory(&RealProbe::new()) }, |apps| {
                    cosmic::Action::App(Message::Loaded(apps))
                })
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
        let search = widget::text_input("Pesquisar apps", &self.query)
            .on_input(Message::Search)
            .width(Length::Fill);

        let refresh = if self.loading {
            button::standard("A ler…")
        } else {
            button::standard("Atualizar").on_press(Message::Refresh)
        };

        let header = widget::row![search, refresh]
            .spacing(12)
            .align_y(Alignment::Center);

        container(
            widget::column![text::title2("App Manager"), header, self.body()]
                .spacing(12)
                .padding(20),
        )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

impl AppManager {
    fn body(&self) -> Element<'_, Message> {
        if self.loading && self.apps.is_empty() {
            return centred("A ler as apps instaladas…");
        }
        if self.apps.is_empty() {
            return centred("Nenhuma app encontrada.");
        }

        let sections = sections(&self.apps, &self.query);
        if sections.is_empty() {
            return centred("Nada corresponde à pesquisa.");
        }

        let mut list = widget::column![].spacing(4);
        for section in sections {
            list = list.push(text::heading(section.title));
            list = list.push(text::caption(format!("{} apps", section.apps.len())));
            for app in &section.apps {
                list = list.push(app_row(app));
            }
        }

        scrollable(list).height(Length::Fill).into()
    }
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
