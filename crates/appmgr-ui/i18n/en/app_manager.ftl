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

preview-title = Removal preview
preview-simulation = This is a simulation. Nothing will be removed.
preview-will-remove = This will remove:
preview-extra = Also removed because they depend on it:
preview-kept = { $count ->
    [one] { $count } automatically installed package is no longer needed, but it stays installed.
   *[other] { $count } automatically installed packages are no longer needed, but they stay installed.
}
preview-flatpak-note = The Flatpak app is removed. Its data in your home folder stays.
preview-appimage-note = The AppImage file is removed. Other files are not touched.
preview-error = The system refused this removal:
preview-shared-libs = Libraries used by other apps stay installed.
planning = Asking the system…
back = Back
