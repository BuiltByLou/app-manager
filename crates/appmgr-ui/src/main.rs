mod i18n;
mod prefs;
mod view_model;

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Alignment, Length, Size};
use cosmic::widget::{self, button, container, scrollable, text};
use cosmic::{Application, Element};

use appmgr_core::inventory::{App as InstalledApp, InstallKind, Origin, build_inventory};
use appmgr_core::leftovers::{Leftover, append_history, find_leftovers, trash_paths};
use appmgr_core::github::{Asset, AssetKind, Repo, Release, fetch_latest, installable_assets};
use appmgr_core::real::RealProbe;
use appmgr_core::install::{
    appimage_integration_paths, cache_dir, deb_package_name, download, install_appimage,
    install_deb, install_flatpak, stable_appimage_path,
};
use appmgr_core::registry::{ManagedApp, Registry};
use appmgr_core::removal::{
    RemovalPlan, authorize, is_authorized, keep_sudo_alive, plan_for, removal_command, run_removal,
};
use appmgr_core::updates::{UpdateCheck, UpdateStatus, evaluate, is_newer};
use appmgr_core::version;
use prefs::Preferences;
use std::collections::{HashMap, HashSet};
use view_model::{SourceTag, format_size, source_tag, visible_rows};

const APP_ID: &str = "io.github.builtbylou.app_manager";

fn main() -> cosmic::iced::Result {
    let settings = Settings::default().size(Size::new(1000.0, 680.0));
    cosmic::app::run::<AppManager>(settings, ())
}

fn source_tag_label(tag: SourceTag) -> String {
    match tag {
        SourceTag::Hub => fl!("tag-hub"),
        SourceTag::GitHub => fl!("tag-github"),
        SourceTag::Local => fl!("tag-local"),
    }
}

fn kind_label(kind: InstallKind) -> String {
    match kind {
        InstallKind::User => fl!("tag-user"),
        InstallKind::PreInstalled => fl!("tag-preinstalled"),
        InstallKind::System => fl!("tag-system"),
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
    Leftovers {
        items: Vec<Leftover>,
    },
    Failed {
        error: String,
    },
    /// Asks which GitHub repository an already installed app came from.
    LinkSource {
        app: InstalledApp,
        url: String,
        error: Option<String>,
    },
    Updating {
        name: String,
    },
    UpdateFailed {
        error: String,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
    Loaded(Vec<InstalledApp>),
    Search(String),
    /// Rows carry their index so the view never clones an id to build a message.
    OpenDetails(usize),

    CloseDetail,
    ToggleProtected,
    OpenMenu(usize),
    CloseMenu,
    StartQuickRemoval(usize),
    StartDetailedRemoval(usize),
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
        result: Result<Vec<Leftover>, String>,
    },
    TrashLeftovers,
    LeftoversTrashed,
    SetConfirmRemovals(bool),
    MaintUrl(String),
    LoadRepo,
    RepoLoaded(Result<(Repo, Release), String>),
    InstallAsset(AssetKind),
    AssetInstalled(Result<String, String>),
    CheckUpdates,
    CheckApp(usize),
    UpdatesChecked(Vec<(String, UpdateCheck)>),
    ApplyUpdate(usize),
    Updated {
        result: Result<UpdateDone, String>,
    },
    LinkSource(usize),
    SourceUrl(String),
    SaveSource,
    OpenUrl(String),
    SetKindFilter(Option<InstallKind>),
    SetSourceFilter(Option<SourceTag>),
    ToggleMaintainer,
}

/// What a finished update leaves behind for the UI.
#[derive(Debug, Clone)]
pub struct UpdateDone {
    pub old_id: String,
    pub new_id: String,
    pub latest: String,
    pub message: String,
}

pub struct AppManager {
    core: Core,
    apps: Vec<InstalledApp>,
    /// Subtitle and icon per app, resolved once when the inventory loads.
    /// Rebuilding them on every redraw is what made long lists feel heavy.
    subtitles: Vec<String>,
    icons: Vec<widget::icon::Handle>,
    /// (classification, source) tag labels per app, resolved with the icons.
    tags: Vec<(String, String)>,
    /// The unified list, recomputed only when the inventory, the search or a
    /// filter changes. Rows are indices into `apps`.
    all_rows: Vec<usize>,
    query: String,
    /// Active tag filters. `None` means "every value of that tag".
    kind_filter: Option<InstallKind>,
    source_filter: Option<SourceTag>,
    /// Whether the "install from GitHub" view is open.
    show_maintainer: bool,
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
    /// GitHub source recorded for apps installed from a release.
    registry: Registry,
    /// Last update check per app id.
    updates: HashMap<String, UpdateCheck>,
    /// Apps with a check in flight.
    checking_updates: HashSet<String>,
    checking_all: bool,
    update_note: String,
    /// Maintainer page state.
    maint_url: String,
    maint_repo: Option<Repo>,
    maint_release: Option<Release>,
    /// Installable assets of `maint_release`, filtered once when it loads.
    maint_assets: Vec<Asset>,
    maint_status: String,
    maint_working: bool,
}

impl Application for AppManager {
    /// Tasks here spawn processes that block (dpkg, apt, curl, gio). A
    /// single worker thread would make them queue behind each other, so the
    /// runtime gets a pool.
    type Executor = cosmic::executor::multi::Executor;
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
                subtitles: Vec::new(),
                icons: Vec::new(),
                tags: Vec::new(),
                all_rows: Vec::new(),
                query: String::new(),
                kind_filter: None,
                source_filter: None,
                show_maintainer: false,
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
                registry: Registry::load(),
                updates: HashMap::new(),
                checking_updates: HashSet::new(),
                checking_all: false,
                update_note: String::new(),
                maint_url: String::new(),
                maint_repo: None,
                maint_release: None,
                maint_assets: Vec::new(),
                maint_status: String::new(),
                maint_working: false,
            },
            Task::batch([load_inventory(), check_auth()]),
        )
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
            Message::Loaded(mut apps) => {
                // A managed AppImage uses a stable filename, so its version is
                // not in the name anymore. The source registry still knows it.
                for app in &mut apps {
                    if let Some(managed) = self.registry.get(&app.id) {
                        // The system is the source of truth for what is
                        // installed. The registry only fills the gap for an
                        // AppImage, whose stable filename carries no version.
                        if app.version.is_none() && !managed.installed_version.is_empty() {
                            app.version = Some(managed.installed_version.clone());
                        }
                        // An app installed from GitHub is always the user's app,
                        // even when it came in as a .deb or a system Flatpak.
                        app.kind = InstallKind::User;
                        app.protected = false;
                    }
                }
                self.apps = apps;
                self.loading = false;
                self.rebuild_app_text();
                self.rebuild_rows();
                if self.selected_app().is_none() {
                    self.selected = None;
                }
                if self.detail_app().is_none() {
                    self.detail = None;
                }
                if self.menu_app().is_none() {
                    self.menu = None;
                }
                Task::none()
            }
            Message::Search(query) => {
                self.query = query;
                self.menu = None;
                self.rebuild_rows();
                if let Some(selected) = self.selected.as_deref()
                    && !self.is_visible(selected)
                {
                    self.selected = None;
                }
                Task::none()
            }
            Message::OpenDetails(index) => {
                self.menu = None;
                let Some(id) = self.apps.get(index).map(|app| app.id.clone()) else {
                    return Task::none();
                };
                self.selected = Some(id.clone());
                self.detail = Some(id.clone());
                // A linked app is checked the first time its details open, so
                // the pane can show the update without hunting for "Check".
                if !self.updates.contains_key(&id)
                    && !self.checking_updates.contains(&id)
                    && let Some(managed) = self.registry.get(&id).cloned()
                {
                    self.checking_updates.insert(id.clone());
                    self.update_note = fl!("update-checking");
                    return check_updates(vec![(id, managed)]);
                }
                Task::none()
            }
            Message::CloseDetail => {
                self.detail = None;
                Task::none()
            }
            Message::ToggleProtected => {
                self.show_protected = !self.show_protected;
                self.rebuild_rows();
                if let Some(selected) = self.selected.as_deref()
                    && !self.is_visible(selected)
                {
                    self.selected = None;
                }
                Task::none()
            }
            Message::OpenMenu(index) => {
                self.menu = self.apps.get(index).map(|app| app.id.clone());
                Task::none()
            }
            Message::CloseMenu => {
                self.menu = None;
                Task::none()
            }
            Message::StartQuickRemoval(index) => {
                self.menu = None;
                let Some(app) = self.removable_at(index) else {
                    return Task::none();
                };
                if self.confirm_removals {
                    self.dialog = Some(Dialog::QuickConfirm { app, dont_ask: false });
                    Task::none()
                } else {
                    self.begin_quick_check(app)
                }
            }
            Message::StartDetailedRemoval(index) => {
                self.menu = None;
                let Some(app) = self.removable_at(index) else {
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
                Ok(items) => {
                    self.registry.remove(&app.id);
                    let _ = self.registry.save();
                    self.updates.remove(&app.id);
                    self.checking_updates.remove(&app.id);
                    if items.is_empty() {
                        if matches!(self.dialog, Some(Dialog::Running)) {
                            self.dialog = None;
                        }
                    } else {
                        self.dialog = Some(Dialog::Leftovers { items });
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
            Message::TrashLeftovers => {
                let Some(Dialog::Leftovers { items }) = self.dialog.take() else {
                    return Task::none();
                };
                self.dialog = Some(Dialog::Running);
                Task::perform(
                    async move {
                        let paths: Vec<String> = items.into_iter().map(|item| item.path).collect();
                        let _ = trash_paths(&paths);
                    },
                    |_| cosmic::Action::App(Message::LeftoversTrashed),
                )
            }
            Message::LeftoversTrashed => {
                self.dialog = None;
                Task::none()
            }
            Message::SetConfirmRemovals(confirm) => {
                self.set_confirm_removals(confirm);
                Task::none()
            }
            Message::MaintUrl(url) => {
                self.maint_url = url;
                Task::none()
            }
            Message::LoadRepo => {
                let url = self.maint_url.clone();
                self.maint_working = true;
                self.maint_status = "Loading release…".to_owned();
                self.maint_repo = None;
                self.maint_release = None;
                self.maint_assets.clear();
                Task::perform(
                    async move {
                        let repo = Repo::parse(&url)?;
                        let json = fetch_latest(&repo)?;
                        let release = Release::parse_json(&json)?;
                        Ok((repo, release))
                    },
                    |result| cosmic::Action::App(Message::RepoLoaded(result)),
                )
            }
            Message::RepoLoaded(result) => {
                self.maint_working = false;
                match result {
                    Ok((repo, release)) => {
                        self.maint_status = format!("Loaded {}", release.tag);
                        self.maint_assets = installable_assets(&release);
                        self.maint_repo = Some(repo);
                        self.maint_release = Some(release);
                    }
                    Err(error) => {
                        self.maint_status = error;
                    }
                }
                Task::none()
            }
            Message::InstallAsset(kind) => {
                let Some(asset) = self.maint_assets.iter().find(|a| a.kind == kind).cloned() else {
                    self.maint_status = "No asset of that kind found.".to_owned();
                    return Task::none();
                };
                let (Some(repo), Some(release)) =
                    (self.maint_repo.clone(), self.maint_release.clone())
                else {
                    self.maint_status = "Load a repository first.".to_owned();
                    return Task::none();
                };
                self.maint_working = true;
                self.maint_status = format!("Downloading {}…", asset.name);
                Task::perform(
                    async move { install_asset(&repo, &release, &asset) },
                    |result| cosmic::Action::App(Message::AssetInstalled(result)),
                )
            }
            Message::AssetInstalled(result) => {
                self.maint_working = false;
                match result {
                    Ok(message) => {
                        self.maint_status = message;
                        load_inventory()
                    }
                    Err(error) => {
                        self.maint_status = error;
                        Task::none()
                    }
                }
            }
            Message::CheckUpdates => {
                let entries = self.managed_entries();
                if entries.is_empty() {
                    self.update_note = fl!("update-none");
                    return Task::none();
                }
                self.checking_all = true;
                for (id, _) in &entries {
                    self.checking_updates.insert(id.clone());
                }
                self.update_note = fl!("update-checking");
                check_updates(entries)
            }
            Message::CheckApp(index) => {
                let Some(app) = self.apps.get(index) else {
                    return Task::none();
                };
                let Some(managed) = self.registry.get(&app.id).cloned() else {
                    return Task::none();
                };
                self.checking_updates.insert(app.id.clone());
                self.update_note = fl!("update-checking");
                check_updates(vec![(app.id.clone(), managed)])
            }
            Message::UpdatesChecked(results) => {
                for (id, check) in &results {
                    self.checking_updates.remove(id);
                    self.updates.insert(id.clone(), check.clone());
                }
                self.checking_all = !self.checking_updates.is_empty();
                self.update_note = self.update_summary();
                Task::none()
            }
            Message::ApplyUpdate(index) => {
                let Some(app) = self.apps.get(index).cloned() else {
                    return Task::none();
                };
                let Some(managed) = self.registry.get(&app.id).cloned() else {
                    return Task::none();
                };
                let Some(check) = self.updates.get(&app.id).cloned() else {
                    return Task::none();
                };
                let (Some(asset), Some(latest)) = (check.asset.clone(), check.latest.clone())
                else {
                    self.dialog = Some(Dialog::UpdateFailed {
                        error: "No installable asset was found for this release.".to_owned(),
                    });
                    return Task::none();
                };
                self.dialog = Some(Dialog::Updating {
                    name: app.name.clone(),
                });
                update_app(app, managed, asset, latest)
            }
            Message::Updated { result } => match result {
                Ok(done) => {
                    self.dialog = None;
                    self.update_note = done.message.clone();
                    self.updates.remove(&done.old_id);
                    self.checking_updates.remove(&done.old_id);
                    self.updates.insert(
                        done.new_id.clone(),
                        UpdateCheck {
                            installed: Some(done.latest.clone()),
                            latest: Some(done.latest.clone()),
                            status: UpdateStatus::UpToDate,
                            asset: None,
                            error: None,
                        },
                    );
                    self.selected = Some(done.new_id.clone());
                    self.detail = Some(done.new_id);
                    self.loading = true;
                    load_inventory()
                }
                Err(error) => {
                    self.dialog = Some(Dialog::UpdateFailed { error });
                    Task::none()
                }
            },
            Message::LinkSource(index) => {
                let Some(app) = self.apps.get(index).cloned() else {
                    return Task::none();
                };
                let url = self
                    .registry
                    .get(&app.id)
                    .map(|managed| format!("https://github.com/{}", managed.repo))
                    .unwrap_or_default();
                self.dialog = Some(Dialog::LinkSource {
                    app,
                    url,
                    error: None,
                });
                Task::none()
            }
            Message::SourceUrl(url) => {
                if let Some(Dialog::LinkSource {
                    url: current,
                    error,
                    ..
                }) = &mut self.dialog
                {
                    *current = url;
                    *error = None;
                }
                Task::none()
            }
            Message::SaveSource => {
                let Some(Dialog::LinkSource { app, url, .. }) = self.dialog.take() else {
                    return Task::none();
                };
                let repo = match Repo::parse(&url) {
                    Ok(repo) => repo,
                    Err(_) => {
                        self.dialog = Some(Dialog::LinkSource {
                            app,
                            url,
                            error: Some(fl!("source-invalid")),
                        });
                        return Task::none();
                    }
                };
                let managed = ManagedApp {
                    repo: repo.slug(),
                    format: managed_kind(app.origin).as_str().to_owned(),
                    asset: self
                        .registry
                        .get(&app.id)
                        .map(|managed| managed.asset.clone())
                        .unwrap_or_default(),
                    installed_version: app.version.clone().unwrap_or_default(),
                    package: app.package.clone(),
                    path: appimage_path(&app),
                };
                self.registry.insert(app.id.clone(), managed.clone());
                if let Err(error) = self.registry.save() {
                    self.dialog = Some(Dialog::LinkSource {
                        app,
                        url,
                        error: Some(error),
                    });
                    return Task::none();
                }
                self.update_note = fl!("source-saved");
                // The row tag may just have changed from Deb/Flatpak to GitHub.
                self.rebuild_app_text();
                self.rebuild_rows();
                self.checking_updates.insert(app.id.clone());
                check_updates(vec![(app.id.clone(), managed)])
            }
            Message::OpenUrl(url) => {
                let _ = std::process::Command::new("xdg-open").arg(url).spawn();
                Task::none()
            }
            Message::SetKindFilter(kind) => {
                self.kind_filter = kind;
                self.rebuild_rows();
                self.prune_selection();
                Task::none()
            }
            Message::SetSourceFilter(origin) => {
                self.source_filter = origin;
                self.rebuild_rows();
                self.prune_selection();
                Task::none()
            }
            Message::ToggleMaintainer => {
                self.show_maintainer = !self.show_maintainer;
                self.selected = None;
                self.detail = None;
                self.menu = None;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let check_label = if self.checking_all {
            fl!("update-checking")
        } else {
            fl!("update-check-all")
        };
        let check = if self.checking_all {
            button::standard(check_label)
        } else {
            button::standard(check_label).on_press(Message::CheckUpdates)
        };
        let install_label = if self.show_maintainer {
            fl!("nav-apps")
        } else {
            fl!("nav-maintainer")
        };
        let header = widget::row![
            text::title3(fl!("app-title")),
            widget::Space::new().width(Length::Fill),
            button::standard(install_label).on_press(Message::ToggleMaintainer),
            check,
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let mut content = widget::column![header].spacing(12);
        if !self.update_note.is_empty() {
            content = content.push(text::caption(self.update_note.clone()));
        }

        if !self.show_maintainer {
            content = content.push(self.search_row());
            content = content.push(self.filter_row());
        }

        let body = self.body();
        if let Some((index, app)) = self.detail_app() {
            content = content.push(
                widget::row![body, self.detail(index, app)]
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
    fn selected_app(&self) -> Option<&InstalledApp> {
        let id = self.selected.as_deref()?;
        self.apps.iter().find(|app| app.id == id)
    }

    fn detail_app(&self) -> Option<(usize, &InstalledApp)> {
        let id = self.detail.as_deref()?;
        let index = self.apps.iter().position(|app| app.id == id)?;
        Some((index, &self.apps[index]))
    }

    fn menu_app(&self) -> Option<&InstalledApp> {
        let id = self.menu.as_deref()?;
        self.apps.iter().find(|app| app.id == id)
    }

    fn removable_at(&self, index: usize) -> Option<InstalledApp> {
        self.apps.get(index).filter(|app| !app.protected).cloned()
    }

    /// Recomputes the unified list. Only called when the inventory, the search
    /// or a filter changes — never from `view`.
    fn rebuild_rows(&mut self) {
        let registry = &self.registry;
        self.all_rows = visible_rows(
            &self.apps,
            &self.query,
            self.kind_filter,
            self.source_filter,
            |app| registry.get(&app.id).is_some(),
        );
    }

    /// Row text and icons, resolved once per inventory load.
    fn rebuild_app_text(&mut self) {
        self.subtitles = self.apps.iter().map(subtitle).collect();
        self.icons = self.apps.iter().map(app_icon).collect();
        self.tags = self
            .apps
            .iter()
            .map(|app| {
                (
                    kind_label(app.kind),
                    source_tag_label(source_tag(app, self.registry.get(&app.id).is_some())),
                )
            })
            .collect();
    }

    /// Registered sources that still match an app in the inventory.
    fn managed_entries(&self) -> Vec<(String, ManagedApp)> {
        let installed: HashSet<&str> = self.apps.iter().map(|app| app.id.as_str()).collect();
        self.registry
            .apps
            .iter()
            .filter(|(id, _)| installed.contains(id.as_str()))
            .map(|(id, managed)| (id.clone(), managed.clone()))
            .collect()
    }

    /// The line above the search bar. It always names the app it is about: a
    /// bare "1 check failed" leaves the user guessing which one broke.
    fn update_summary(&self) -> String {
        let mut available = Vec::new();
        let mut unknown = Vec::new();
        let mut failed = Vec::new();

        for (id, check) in &self.updates {
            let name = self
                .apps
                .iter()
                .find(|app| &app.id == id)
                .map(|app| app.name.clone())
                .unwrap_or_else(|| id.clone());
            match check.status {
                UpdateStatus::Available => available.push(name),
                UpdateStatus::Unknown => unknown.push(name),
                UpdateStatus::Failed => {
                    failed.push((name, check.error.clone().unwrap_or_default()));
                }
                UpdateStatus::UpToDate => {}
            }
        }
        available.sort();
        unknown.sort();
        failed.sort_by(|left, right| left.0.cmp(&right.0));

        let mut parts = Vec::new();
        if !available.is_empty() {
            parts.push(fl!(
                "update-available-note",
                count = (available.len() as i64),
                names = available.join(", ")
            ));
        }
        if !unknown.is_empty() {
            parts.push(fl!(
                "update-unknown-note",
                count = (unknown.len() as i64),
                names = unknown.join(", ")
            ));
        }
        match failed.len() {
            0 => {}
            1 => {
                let (name, error) = &failed[0];
                parts.push(fl!(
                    "update-failed-note",
                    name = name.clone(),
                    error = shorten(error, 140)
                ));
            }
            count => {
                let names = failed
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                parts.push(fl!(
                    "update-failed-many",
                    count = (count as i64),
                    names = names
                ));
            }
        }

        if parts.is_empty() {
            fl!("update-all-current")
        } else {
            parts.join(" · ")
        }
    }

    fn is_checking(&self, id: &str) -> bool {
        self.checking_updates.contains(id)
    }

    /// Drops a selection or details panel that a filter just hid.
    fn prune_selection(&mut self) {
        if let Some(selected) = self.selected.clone()
            && !self.is_visible(&selected)
        {
            self.selected = None;
        }
        if let Some(detail) = self.detail.clone()
            && !self.is_visible(&detail)
        {
            self.detail = None;
        }
    }

    /// Whether an app is currently listed after search and filters.
    fn is_visible(&self, id: &str) -> bool {
        let Some(index) = self.apps.iter().position(|app| app.id == id) else {
            return false;
        };
        self.all_rows.contains(&index)
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

    fn body(&self) -> Element<'_, Message> {
        if self.loading && self.apps.is_empty() {
            return centred(fl!("loading-apps"));
        }
        if self.show_maintainer {
            return self.maintainer_page();
        }
        self.list_page()
    }

    /// Tag filters, one row per tag family. Picking the active chip clears it.
    fn filter_row(&self) -> Element<'_, Message> {
        let mut kinds = widget::row![].spacing(6);
        kinds = kinds.push(self.kind_chip(None, fl!("tag-all")));
        for (kind, label) in [
            (InstallKind::User, fl!("tag-user")),
            (InstallKind::PreInstalled, fl!("tag-preinstalled")),
            (InstallKind::System, fl!("tag-system")),
        ] {
            kinds = kinds.push(self.kind_chip(Some(kind), label));
        }

        let mut sources = widget::row![].spacing(6);
        sources = sources.push(self.source_chip(None, fl!("tag-any-source")));
        for (tag, label) in [
            (SourceTag::Hub, fl!("tag-hub")),
            (SourceTag::GitHub, fl!("tag-github")),
            (SourceTag::Local, fl!("tag-local")),
        ] {
            sources = sources.push(self.source_chip(Some(tag), label));
        }

        widget::column![kinds.wrap(), sources.wrap()]
            .spacing(6)
            .into()
    }

    fn kind_chip(&self, kind: Option<InstallKind>, label: String) -> Element<'_, Message> {
        let active = self.kind_filter == kind;
        let next = if active { None } else { kind };
        if active {
            button::suggested(label)
                .on_press(Message::SetKindFilter(next))
                .into()
        } else {
            button::text(label)
                .on_press(Message::SetKindFilter(next))
                .into()
        }
    }

    fn source_chip(&self, tag: Option<SourceTag>, label: String) -> Element<'_, Message> {
        let active = self.source_filter == tag;
        let next = if active { None } else { tag };
        if active {
            button::suggested(label).on_press(Message::SetSourceFilter(next)).into()
        } else {
            button::text(label).on_press(Message::SetSourceFilter(next)).into()
        }
    }

    /// One row, borrowing its app, its precomputed subtitle and its icon.
    fn row(&self, index: usize) -> Element<'_, Message> {
        let Some(app) = self.apps.get(index) else {
            return widget::Space::new().into();
        };
        let subtitle = self.subtitles.get(index).map(String::as_str).unwrap_or("");
        let icon = self
            .icons
            .get(index)
            .cloned()
            .unwrap_or_else(|| widget::icon::from_name("application-x-executable").handle());
        let tags = self.tags.get(index);
        app_row(
            app,
            RowView {
                subtitle,
                kind_tag: tags.map(|(kind, _)| kind.as_str()).unwrap_or(""),
                source_tag: tags.map(|(_, source)| source.as_str()).unwrap_or(""),
                icon,
                selected: self.is_selected(&app.id),
                menu_open: self.is_menu_open(&app.id),
                update_latest: self.updates.get(&app.id).and_then(|check| {
                    let latest = check.latest.as_deref()?;
                    is_newer(app.version.as_deref(), latest).then_some(latest)
                }),
            },
            index,
        )
    }

    /// One list for every app. Locked (protected) apps stay at the bottom,
    /// behind a toggle, so they never get in the way of a quick removal.
    fn list_page(&self) -> Element<'_, Message> {
        let (removable, protected): (Vec<usize>, Vec<usize>) = self
            .all_rows
            .iter()
            .copied()
            .partition(|&index| !self.apps[index].protected);

        let mut list = widget::column![].spacing(4);
        list = list.push(text::caption(fl!(
            "apps-count",
            count = (self.all_rows.len() as i64)
        )));

        if self.all_rows.is_empty() {
            list = list.push(text::body(self.empty_message()));
        }

        for &index in &removable {
            list = list.push(self.row(index));
        }

        if !protected.is_empty() {
            let count = protected.len() as i64;
            let label = if self.show_protected {
                fl!("hide-protected", count = count)
            } else {
                fl!("show-protected", count = count)
            };
            list = list.push(
                widget::row![button::text(label).on_press(Message::ToggleProtected)]
                    .padding([8, 0, 8, 0]),
            );
            if self.show_protected {
                for &index in &protected {
                    list = list.push(self.row(index));
                }
            }
        }

        scrollable(list).height(Length::Fill).into()
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
            Dialog::Leftovers { items } => {
                let mut list = widget::column![].spacing(2);
                for item in items {
                    list = list.push(text::caption(item.path.clone()));
                }
                window = window
                    .body(fl!("leftovers-title"))
                    .control(text::caption(fl!("leftovers-body")))
                    .control(scrollable(list).height(Length::Fixed(220.0)))
                    .primary_action(
                        button::destructive(fl!("leftovers-trash"))
                            .on_press(Message::TrashLeftovers),
                    )
                    .secondary_action(
                        button::standard(fl!("leftovers-skip")).on_press(Message::CancelDialog),
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
            Dialog::LinkSource { app, url, error } => {
                let input: Element<'_, Message> =
                    widget::text_input(fl!("source-url-placeholder"), url)
                        .on_input(Message::SourceUrl)
                        .on_submit(|_| Message::SaveSource)
                        .into();
                window = window
                    .title(fl!("source-title"))
                    .body(fl!("source-help", name = app.name.clone()))
                    .control(input);
                if let Some(error) = error {
                    window = window.control(text::caption(error.clone()));
                }
                window = window
                    .primary_action(
                        button::suggested(fl!("source-save")).on_press(Message::SaveSource),
                    )
                    .secondary_action(
                        button::standard(fl!("remove-cancel")).on_press(Message::CancelDialog),
                    );
            }
            Dialog::Updating { name } => {
                window = window
                    .title(fl!("update-title"))
                    .body(fl!("update-running", name = name.clone()));
            }
            Dialog::UpdateFailed { error } => {
                window = window
                    .title(fl!("update-title"))
                    .body(fl!("update-failed"))
                    .control(text::caption(error.clone()))
                    .primary_action(
                        button::standard(fl!("remove-close")).on_press(Message::CancelDialog),
                    );
            }
        }

        Some(window.into())
    }

    fn maintainer_page(&self) -> Element<'_, Message> {
        let mut content = widget::column![text::title3(fl!("nav-maintainer"))].spacing(12);
        let url_input = widget::text_input(fl!("maint-url-placeholder"), &self.maint_url)
            .on_input(Message::MaintUrl)
            .width(Length::Fill);
        let load = if self.maint_working {
            button::standard(fl!("maint-loading"))
        } else {
            button::standard(fl!("maint-load")).on_press(Message::LoadRepo)
        };
        content = content.push(widget::row![url_input, load].spacing(12).align_y(Alignment::Center));

        if !self.maint_status.is_empty() {
            content = content.push(text::body(self.maint_status.clone()));
        }

        if let Some(release) = &self.maint_release {
            content = content.push(text::title3(format!("{} {}", fl!("maint-release"), release.tag)));
            // Derive a display name from the repo URL.
            let name = self
                .maint_url
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("App")
                .to_owned();

            let heading = widget::row![
                widget::icon::from_name("application-x-executable").size(48),
                widget::column![text::title3(name), text::caption(release.tag.clone())].spacing(2),
            ]
            .spacing(12)
            .align_y(Alignment::Center);

            // One button per compatible asset, stacked. A wrapping row inside
            // a shrinking card lays out with no usable width, which is what
            // made the install buttons disappear.
            let mut buttons = widget::column![].spacing(6).width(Length::Fill);
            for asset in &self.maint_assets {
                let label = match asset.kind {
                    AssetKind::Deb => fl!("maint-install-deb"),
                    AssetKind::AppImage => fl!("maint-install-appimage"),
                    AssetKind::Flatpak => fl!("maint-install-flatpak"),
                    AssetKind::Other => continue,
                };
                buttons = buttons.push(
                    button::standard(label)
                        .width(Length::Fill)
                        .on_press(Message::InstallAsset(asset.kind)),
                );
            }
            if self.maint_assets.is_empty() {
                buttons = buttons.push(text::body(fl!("maint-no-assets")));
            }

            let card = widget::column![heading, buttons].spacing(12);
            content = content.push(container(card.padding(16)).width(Length::Fill));
        }

        container(content.padding(20).width(Length::Fill).height(Length::Fill)).into()
    }

    fn detail(&self, index: usize, app: &InstalledApp) -> Element<'_, Message> {
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
        // What is installed first, then what could be installed. Never the
        // other way around: a linked repository must not make the app claim a
        // version it does not have.
        fields = fields.push(field(fl!("detail-version"), version_label(app)));
        let managed = self.registry.get(&app.id);
        let tag = source_tag(app, managed.is_some());
        fields = fields.push(field(
            fl!("detail-source"),
            match (managed, tag) {
                (Some(managed), _) => managed.repo.clone(),
                (None, SourceTag::Hub) => fl!("source-hub"),
                (None, _) => fl!("source-not-linked"),
            },
        ));
        if let Some(check) = self.updates.get(&app.id) {
            // Installed and available are both shown, so the comparison is
            // visible even when they are equal.
            if let Some(latest) = &check.latest {
                fields = fields.push(field(
                    fl!("detail-available"),
                    version::display(latest).to_owned(),
                ));
            }
            fields = fields.push(field(fl!("detail-update"), update_status_label(check)));
        }
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

        // The update action sits right under the version it would change, so
        // it is never buried at the bottom of the pane.
        if let Some(check) = self.updates.get(&app.id)
            && check.asset.is_some()
            && is_newer(app.version.as_deref(), check.latest.as_deref().unwrap_or(""))
        {
            content = content.push(
                button::suggested(fl!("update-apply")).on_press(Message::ApplyUpdate(index)),
            );
        }

        // A hub app is updated by apt or Flathub, so it has nothing to link.
        // A GitHub app can be re-pointed; a local file can be linked to start
        // receiving updates here.
        let mut source_actions = widget::row![].spacing(8);
        match (tag, managed) {
            (SourceTag::Hub, _) => {
                content = content.push(text::caption(fl!("source-hub-note")));
            }
            (SourceTag::GitHub, Some(managed)) => {
                source_actions = source_actions.push(
                    button::link(managed.repo.clone()).on_press(Message::OpenUrl(format!(
                        "https://github.com/{}",
                        managed.repo
                    ))),
                );
                source_actions = source_actions
                    .push(button::text(fl!("source-edit")).on_press(Message::LinkSource(index)));
                if self.is_checking(&app.id) {
                    source_actions = source_actions.push(button::standard(fl!("update-checking")));
                } else {
                    source_actions = source_actions
                        .push(button::text(fl!("update-check")).on_press(Message::CheckApp(index)));
                }
                content = content.push(source_actions);
            }
            _ => {
                content = content
                    .push(text::caption(fl!("source-local-note")));
                content = content.push(
                    widget::row![button::standard(fl!("source-link"))
                        .on_press(Message::LinkSource(index))]
                    .spacing(8),
                );
            }
        }

        if let Some(check) = self.updates.get(&app.id) {
            if let (Some(latest), Some(managed)) = (&check.latest, managed)
                && let Ok(repo) = Repo::parse(&managed.repo)
            {
                content = content.push(
                    button::link(fl!("release-notes"))
                        .on_press(Message::OpenUrl(repo.release_url(latest))),
                );
            }
            if let Some(error) = &check.error {
                content = content.push(text::caption(error.clone()));
            }
        }

        let remove = if app.protected {
            widget::column![
                button::destructive(fl!("remove")),
                text::caption(fl!("remove-blocked")),
            ]
            .spacing(6)
        } else {
            widget::column![button::destructive(fl!("remove"))
                .on_press(Message::StartDetailedRemoval(index))]
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
    Task::perform(
        async {
            // A debug build can authorize from a developer's own environment,
            // so no password is ever written into the source.
            #[cfg(debug_assertions)]
            {
                if let Ok(password) = std::env::var("APPMGR_DEBUG_PASSWORD")
                    && !password.is_empty()
                    && authorize(&password).is_ok()
                {
                    return true;
                }
            }
            is_authorized()
        },
        |authorized| cosmic::Action::App(Message::AuthChecked(authorized)),
    )
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
/// the app must stay responsive. History and leftover detection happen there
/// too: both touch the disk and have no business on the UI thread.
fn remove_app(app: InstalledApp) -> Task<Message> {
    Task::perform(
        async move {
            let (sender, receiver) = cosmic::iced::futures::channel::oneshot::channel();
            let thread_app = app.clone();
            std::thread::spawn(move || {
                let result = run_removal(&thread_app).map(|()| {
                    let home =
                        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
                    append_history(
                        &home,
                        &thread_app.name,
                        &format!("{:?}", thread_app.origin),
                        "removed",
                    );
                    find_leftovers(&thread_app, &home)
                });
                let _ = sender.send(result);
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

/// A small pill under the app name. The classification tag uses the accent
/// tint so the eye can separate "mine" from "part of the system" at a glance.
fn tag_badge<'a>(label: &'a str, accent: bool) -> Element<'a, Message> {
    let class = if accent {
        cosmic::theme::Container::custom(|theme| {
            let cosmic = theme.cosmic();
            let mut background: cosmic::iced::Color = cosmic.accent.base.into();
            background.a = 0.22;
            cosmic::iced::widget::container::Style {
                background: Some(cosmic::iced::Background::Color(background)),
                border: cosmic::iced::Border {
                    radius: cosmic.corner_radii.radius_xs.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
    } else {
        cosmic::theme::Container::Card
    };
    container(text::caption(label.to_owned()))
        .padding([1, 6])
        .class(class)
        .into()
}

fn kind_tag_badge<'a>(label: &'a str) -> Element<'a, Message> {
    tag_badge(label, true)
}

fn source_tag_badge<'a>(label: &'a str) -> Element<'a, Message> {
    tag_badge(label, false)
}

/// One app row with its three-dot menu. The row itself is not clickable:
/// details open from the menu, so removing stays a deliberate action.
///
/// The subtitle and the icon arrive precomputed and the row is identified by
/// its index, so a redraw neither formats texts nor clones ids.
/// Everything a row needs beyond the app itself, resolved once per inventory
/// load so a redraw does not format text or look anything up.
struct RowView<'a> {
    subtitle: &'a str,
    kind_tag: &'a str,
    source_tag: &'a str,
    icon: widget::icon::Handle,
    selected: bool,
    menu_open: bool,
    /// The version waiting upstream, when there is one.
    update_latest: Option<&'a str>,
}

fn app_row<'a>(
    app: &'a InstalledApp,
    view: RowView<'a>,
    index: usize,
) -> Element<'a, Message> {
    let RowView {
        subtitle,
        kind_tag,
        source_tag,
        icon,
        selected,
        menu_open,
        update_latest,
    } = view;
    let icon = widget::icon::icon(icon).size(32);

    let title = text(app.name.as_str()).size(15);
    let subtitle = text(subtitle).size(12);
    let tags = widget::row![kind_tag_badge(kind_tag), source_tag_badge(source_tag)]
        .spacing(4)
        .align_y(Alignment::Center);

    let labels = widget::column![title, subtitle, tags]
        .spacing(2)
        .width(Length::Fill);

    let mut row = widget::row![icon, labels]
        .spacing(12)
        .align_y(Alignment::Center);
    if app.protected {
        row = row.push(widget::icon::from_name("changes-prevent-symbolic").size(16));
    }
    if let Some(latest) = update_latest {
        row = row.push(
            container(text::caption(format!(
                "{} · {}",
                fl!("update-badge"),
                version::display(latest)
            )))
            .padding([2, 8])
            .class(cosmic::theme::Container::Card),
        );
    }

    let menu_icon = button::icon(widget::icon::from_name("open-menu-symbolic").size(16))
        .on_press(Message::OpenMenu(index));
    let menu_button: Element<'a, Message> = if menu_open {
        widget::popover(menu_icon)
            .popup(menu_popup(app, index))
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

    let row: Element<'a, Message> = container(row)
        .padding(8)
        .width(Length::Fill)
        .class(style)
        .into();

    // Clicking the row opens its details. The three-dot menu sits inside this
    // area and handles its own clicks first, so removal stays deliberate.
    widget::mouse_area(row)
        .on_press(Message::OpenDetails(index))
        .into()
}

fn menu_popup<'a>(app: &'a InstalledApp, index: usize) -> Element<'a, Message> {
    let details: Element<'a, Message> = button::text(fl!("details"))
        .on_press(Message::OpenDetails(index))
        .into();

    let mut menu = widget::column![details].spacing(4).width(Length::Fixed(220.0));
    if app.protected {
        menu = menu.push(text::caption(fl!("remove-blocked")));
    } else {
        let remove: Element<'a, Message> = button::destructive(fl!("remove"))
            .on_press(Message::StartQuickRemoval(index))
            .into();
        menu = menu.push(remove);
    }

    container(menu)
        .padding(6)
        .class(cosmic::theme::Container::Dropdown)
        .into()
}

/// The one line every row shows under the app name.
fn subtitle(app: &InstalledApp) -> String {
    format!(
        "{} · {} · {}",
        origin_label(app.origin),
        version_label(app),
        format_size(app.size_bytes),
    )
}

/// Resolves an app's icon once, instead of on every redraw.
fn app_icon(app: &InstalledApp) -> widget::icon::Handle {
    match app.icon.as_deref() {
        Some(value) if value.contains('/') && std::path::Path::new(value).is_file() => {
            widget::icon::from_path(std::path::PathBuf::from(value))
        }
        Some(value) if !value.is_empty() => widget::icon::from_name(value).handle(),
        _ => widget::icon::from_name("application-x-executable").handle(),
    }
}

fn origin_label(origin: Origin) -> String {
    match origin {
        Origin::Apt => fl!("origin-apt"),
        Origin::Flatpak => fl!("origin-flatpak"),
        Origin::AppImage => fl!("origin-appimage"),
    }
}

/// Collapses an error into one short line, so the note above the list stays a
/// note instead of a stack of curl output.
fn shorten(text: &str, max: usize) -> String {
    let single = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if single.chars().count() <= max {
        return single;
    }
    let mut cut: String = single.chars().take(max).collect();
    cut.push('…');
    cut
}

fn version_label(app: &InstalledApp) -> String {
    app.version
        .as_deref()
        .map(version::display)
        .map(str::to_owned)
        .unwrap_or_else(|| fl!("unknown-version"))
}

fn managed_kind(origin: Origin) -> AssetKind {
    match origin {
        Origin::Apt => AssetKind::Deb,
        Origin::Flatpak => AssetKind::Flatpak,
        Origin::AppImage => AssetKind::AppImage,
    }
}

fn appimage_path(app: &InstalledApp) -> Option<String> {
    if app.origin == Origin::AppImage {
        app.id.strip_prefix("appimage:").map(str::to_owned)
    } else {
        None
    }
}

fn update_status_label(check: &UpdateCheck) -> String {
    match check.status {
        UpdateStatus::Unknown => fl!("update-unknown"),
        UpdateStatus::UpToDate => fl!("update-up-to-date"),
        UpdateStatus::Available => fl!("update-available"),
        UpdateStatus::Failed => fl!("update-check-failed-short"),
    }
}

fn home_dir() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}

/// Checks every registered source, in order. A failed repository is reported
/// for that app only; the rest still get checked.
fn check_updates(entries: Vec<(String, ManagedApp)>) -> Task<Message> {
    Task::perform(
        async move {
            entries
                .into_iter()
                .map(|(id, managed)| (id, check_entry(&managed)))
                .collect::<Vec<_>>()
        },
        |results| cosmic::Action::App(Message::UpdatesChecked(results)),
    )
}

fn check_entry(managed: &ManagedApp) -> UpdateCheck {
    let installed = (!managed.installed_version.is_empty())
        .then_some(managed.installed_version.as_str());
    let Some(kind) = AssetKind::parse(&managed.format) else {
        return UpdateCheck::failed(installed, "unknown install format".to_owned());
    };
    let repo = match Repo::parse(&managed.repo) {
        Ok(repo) => repo,
        Err(error) => return UpdateCheck::failed(installed, error),
    };
    let release = match fetch_latest(&repo).and_then(|json| Release::parse_json(&json)) {
        Ok(release) => release,
        Err(error) => return UpdateCheck::failed(installed, error),
    };
    let previous = (!managed.asset.is_empty()).then_some(managed.asset.as_str());
    evaluate(installed, &release, &kind, previous)
}

/// Downloads and installs a release asset, and records where it came from.
fn install_asset(repo: &Repo, release: &Release, asset: &Asset) -> Result<String, String> {
    let destination = cache_dir().join(&asset.name);
    let outcome = (|| -> Result<(String, ManagedApp), String> {
        download(&asset.url, &destination)?;
        let (id, package, path) = match asset.kind {
            AssetKind::Deb => {
                let package = deb_package_name(&destination)?;
                install_deb(&destination)?;
                (format!("apt:{package}"), Some(package), None)
            }
            AssetKind::AppImage => {
                let target = stable_appimage_path(&repo.name);
                install_appimage(&destination, &target)?;
                (
                    format!("appimage:{}", target.display()),
                    None,
                    Some(target.display().to_string()),
                )
            }
            AssetKind::Flatpak => {
                let package = install_flatpak(&destination)?;
                (format!("flatpak:{package}"), Some(package), None)
            }
            AssetKind::Other => return Err("this asset cannot be installed".to_owned()),
        };
        let managed = ManagedApp {
            repo: repo.slug(),
            format: asset.kind.as_str().to_owned(),
            asset: asset.name.clone(),
            installed_version: release.tag.clone(),
            package,
            path,
        };
        Ok((id, managed))
    })();
    // The download is only a means to install; do not leave a second copy.
    let _ = std::fs::remove_file(&destination);
    let (id, managed) = outcome?;

    let mut registry = Registry::load();
    registry.insert(id, managed);
    let note = match registry.save() {
        Ok(()) => String::new(),
        Err(error) => format!(" (the source could not be saved: {error})"),
    };
    Ok(format!("Installed {}{note}", asset.name))
}

/// Applies an update in place, keeping user data and the same install format.
fn update_app(
    app: InstalledApp,
    managed: ManagedApp,
    asset: Asset,
    latest: String,
) -> Task<Message> {
    Task::perform(
        async move { apply_update(&app, &managed, &asset, &latest) },
        |result| cosmic::Action::App(Message::Updated { result }),
    )
}

fn apply_update(
    app: &InstalledApp,
    managed: &ManagedApp,
    asset: &Asset,
    latest: &str,
) -> Result<UpdateDone, String> {
    let repo = Repo::parse(&managed.repo)?;
    let destination = cache_dir().join(&asset.name);
    let outcome = (|| -> Result<(String, ManagedApp), String> {
        download(&asset.url, &destination)?;
        let (new_id, package, path) = match asset.kind {
            AssetKind::Deb => {
                let package = deb_package_name(&destination)?;
                if let Some(previous) = &managed.package
                    && previous != &package
                {
                    return Err(format!(
                        "the package name changed from {previous} to {package}; update it by hand"
                    ));
                }
                install_deb(&destination)?;
                (format!("apt:{package}"), Some(package), None)
            }
            AssetKind::AppImage => {
                let target = stable_appimage_path(&repo.name);
                install_appimage(&destination, &target)?;
                (
                    format!("appimage:{}", target.display()),
                    None,
                    Some(target.display().to_string()),
                )
            }
            AssetKind::Flatpak => {
                let package = install_flatpak(&destination)?;
                (format!("flatpak:{package}"), Some(package), None)
            }
            AssetKind::Other => return Err("this asset cannot be installed".to_owned()),
        };
        let mut next = managed.clone();
        next.asset = asset.name.clone();
        next.installed_version = latest.to_owned();
        next.package = package;
        next.path = path;
        Ok((new_id, next))
    })();
    let _ = std::fs::remove_file(&destination);
    let (new_id, next) = outcome?;

    // Only after the new AppImage is in place does the old integration go to
    // the trash. If anything failed above, the previous app is untouched.
    if asset.kind == AssetKind::AppImage {
        let old_path = managed
            .path
            .clone()
            .or_else(|| app.id.strip_prefix("appimage:").map(str::to_owned));
        if let Some(old_path) = old_path
            && next.path.as_deref() != Some(old_path.as_str())
        {
            let paths: Vec<String> =
                appimage_integration_paths(std::path::Path::new(&old_path), &home_dir())
                    .into_iter()
                    .filter(|path| path.exists())
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
            let _ = trash_paths(&paths);
        }
    }

    let mut registry = Registry::load();
    if new_id != app.id {
        registry.remove(&app.id);
    }
    registry.insert(new_id.clone(), next);
    let note = match registry.save() {
        Ok(()) => String::new(),
        Err(error) => format!(" (the source could not be saved: {error})"),
    };
    append_history(&home_dir(), &app.name, "GitHub", "updated");

    Ok(UpdateDone {
        old_id: app.id.clone(),
        new_id,
        latest: latest.to_owned(),
        message: format!(
            "Updated {} to {}{note}",
            app.name,
            version::display(latest)
        ),
    })
}
