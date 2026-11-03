# English is the source language. Missing messages in other locales fall back
# to these. Keep keys stable: they are referenced from the code.

app-title = App Manager

nav-user = User Installed
nav-system = System Installed
nav-maintainer = Install from GitHub
nav-apps = Installed apps

tag-all = All
tag-user = User
tag-preinstalled = Pre-installed
tag-system = System
tag-any-source = Any source
tag-hub = Hub
tag-github = GitHub
tag-local = Local

search-placeholder = Search apps
refresh = Refresh
refreshing = Reading…

loading-apps = Reading installed apps…
maintainer-soon = Installing and updating apps from a GitHub repository is the next phase. For now, use the installed apps.
maint-url-placeholder = https://github.com/owner/repo
maint-load = Load latest release
maint-loading = Loading…
maint-install = Install
maint-release = Release
maint-install-deb = Install .deb
maint-install-appimage = Install .AppImage
maint-install-flatpak = Install .flatpak
maint-no-assets = This release has no installable build for this machine.

apps-count = { $count ->
    [one] { $count } app
   *[other] { $count } apps
}

no-apps = No apps in this group.
no-results = Nothing matches your search.

protected-title = Protected
protected-caption = Part of the system; removing may break it.
removable-title = Removable
removable-caption = Installed by the system, but safe to remove.
show-protected = Show locked ({ $count })
hide-protected = Hide locked ({ $count })

origin-apt = APT
origin-flatpak = Flatpak
origin-appimage = AppImage
unknown-version = unknown version

details = Details
detail-origin = Origin
detail-version = Version
detail-size = Size
detail-package = Package
detail-status = Status
status-protected = Protected
status-removable = Removable
remove = Remove
remove-blocked = Part of the system. It cannot be removed here.
ask-before-removing = Ask before removing

auth-title = Administrator access
auth-body = App Manager needs administrator access to remove system apps. Enter your password once and it will not be asked again while the app is open.
auth-body-remove = Enter your administrator password to continue. It will not be asked again while the app is open.
auth-placeholder = Password
auth-checking = Checking…
auth-failed = Wrong password, or this account cannot use sudo.
auth-continue = Continue
auth-skip = Not now

remove-title = Remove app
remove-quick-body = { $name } will be removed. Libraries used by other apps stay installed.
remove-dont-ask = Don't ask again
remove-checking = Checking what will be removed…
remove-planning = Asking the system what this will remove…
remove-body = { $name } will be removed.
remove-package = Package
remove-extra = Also removed because they depend on it:
remove-kept = { $count ->
    [one] { $count } automatically installed package is no longer needed, but it stays installed.
   *[other] { $count } automatically installed packages are no longer needed, but they stay installed.
}
remove-shared-libs = Libraries used by other apps stay installed.
remove-flatpak-note = The Flatpak app is removed. Its data in your home folder stays.
remove-appimage-note = The AppImage file is moved to the trash. Other files are not touched.
remove-confirm = Remove
remove-cancel = Cancel
remove-running = Removing… 
remove-running-note = You can close this window; the removal continues in the background.
remove-failed = The removal failed:
remove-refused = The system refused this removal:
remove-close = Close
leftovers-title = Leftover files
leftovers-body = These files might be left behind. Send them to the trash?
leftovers-trash = Move to trash
leftovers-skip = Keep them

detail-source = Source
detail-available = Available version
detail-update = Updates
source-not-linked = Not linked
source-hub = Public hub (apt/Flathub)
source-hub-note = This app is updated by its hub, so there is nothing to link here.
source-local-note = This file has no hub behind it. Link the project's GitHub repository to get updates here.
source-link = Link repository
source-edit = Edit source
source-title = GitHub repository
source-help = App Manager will check { $name } against the latest release of this repository.
source-url-placeholder = https://github.com/owner/repo
source-save = Save
source-invalid = Enter a valid GitHub repository URL.
source-saved = Source saved. Checking for updates…
release-notes = Release notes

update-check = Check
update-check-all = Check all updates
update-checking = Checking for updates…
update-available-note = { $count ->
    [one] 1 update available: { $names }
   *[other] { $count } updates available: { $names }
}
update-all-current = All linked apps are up to date.
update-failed-note = Could not check { $name }: { $error }
update-failed-many = { $count ->
    [one] 1 check failed: { $names }
   *[other] { $count } checks failed: { $names }
}
update-check-failed-short = Check failed
update-unknown = Unknown
update-unknown-note = { $count ->
    [one] 1 version could not be compared: { $names }
   *[other] { $count } versions could not be compared: { $names }
}
update-up-to-date = Up to date
update-available = Update available
update-badge = Update
update-apply = Update
update-title = Update
update-running = Updating { $name }…
update-failed = The update failed:
update-none = No GitHub source is linked yet. Open an app's details to link one.
