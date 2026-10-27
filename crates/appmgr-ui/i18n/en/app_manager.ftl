# English is the source language. Missing messages in other locales fall back
# to these. Keep keys stable: they are referenced from the code.

app-title = App Manager

nav-user = User Installed
nav-system = System Installed
nav-maintainer = Maintainer

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
