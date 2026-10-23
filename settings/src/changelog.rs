use adw::gtk;
use adw::prelude::*;
use libadwaita as adw;

const CHANGELOG: &str = include_str!("../../CHANGELOG.md");

struct Release<'a> {
    version: &'a str,
    changes: Vec<&'a str>,
}

fn releases() -> Vec<Release<'static>> {
    let mut releases = Vec::new();
    for line in CHANGELOG.lines() {
        if let Some(version) = line.strip_prefix("## ") {
            releases.push(Release {
                version,
                changes: Vec::new(),
            });
        } else if let Some(change) = line.strip_prefix("- ") {
            if let Some(release) = releases.last_mut() {
                release.changes.push(change);
            }
        }
    }
    releases
}

pub fn show(parent: &gtk::Window) {
    let dialog = adw::PreferencesDialog::builder()
        .title("Changelog")
        .content_width(640)
        .content_height(600)
        .build();
    let page = adw::PreferencesPage::new();
    page.set_title("Release history");

    for release in releases() {
        let group = adw::PreferencesGroup::new();
        group.set_title(&format!("Version {}", release.version));
        for change in release.changes {
            let row = adw::ActionRow::new();
            row.set_title(change);
            row.set_title_lines(0);
            group.add(&row);
        }
        page.add(&group);
    }

    dialog.add(&page);
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::releases;

    #[test]
    fn history_includes_every_release() {
        let releases = releases();
        let versions: Vec<_> = releases.iter().map(|release| release.version).collect();
        assert_eq!(versions.first().copied(), Some(env!("CARGO_PKG_VERSION")));
        assert!(versions.contains(&"0.1.0"));
        assert!(releases.iter().all(|release| !release.changes.is_empty()));
    }
}
