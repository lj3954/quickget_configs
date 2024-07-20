use crate::{
    store_data::{Arch, ChecksumSeparation, Config, Distro, Source, WebSource},
    utils::{arch_from_str, capture_page, FedoraRelease, GatherData},
};
use quickemu::config::DiskFormat;
use quickget::data_structures::{ArchiveFormat, Disk};
use regex::Regex;
use std::sync::Arc;

const ALMA_MIRROR: &str = "https://repo.almalinux.org/almalinux/";

pub struct Alma;
impl Distro for Alma {
    const NAME: &'static str = "alma";
    const PRETTY_NAME: &'static str = "AlmaLinux";
    const HOMEPAGE: Option<&'static str> = Some("https://almalinux.org/");
    const DESCRIPTION: Option<&'static str> = Some("Community owned and governed, forever-free enterprise Linux distribution, focused on long-term stability, providing a robust production-grade platform. AlmaLinux OS is binary compatible with RHEL®.");
    async fn generate_configs() -> Option<Vec<Config>> {
        let releases = capture_page(ALMA_MIRROR).await?;

        let releases_regex = Regex::new(r#"<a href="([0-9]+)/""#).unwrap();
        let iso_regex = Arc::new(Regex::new(r#"<a href="(AlmaLinux-[0-9]+-latest-(?:x86_64|aarch64)-([^-]+).iso)">"#).unwrap());

        let futures = releases_regex.captures_iter(&releases).flat_map(|r| {
            let release = r[1].to_string();
            [Arch::x86_64, Arch::aarch64]
                .iter()
                .map(|arch| {
                    let release = release.clone();
                    let iso_regex = iso_regex.clone();
                    let mirror = format!("{ALMA_MIRROR}{release}/isos/{arch}/");

                    async move {
                        let page = capture_page(&mirror).await?;
                        let checksums = ChecksumSeparation::Sha256Regex.build(&format!("{mirror}CHECKSUM")).await;

                        Some(
                            iso_regex
                                .captures_iter(&page)
                                .filter(|c| !c.get(0).unwrap().as_str().ends_with(".manifest"))
                                .map(|c| {
                                    let iso = c[1].to_string();
                                    let edition = c[2].to_string();
                                    let url = format!("{mirror}{iso}");
                                    let checksum = checksums.as_ref().and_then(|cs| cs.get(&iso)).cloned();
                                    Config {
                                        release: Some(release.to_string()),
                                        edition: Some(edition),
                                        arch: arch.clone(),
                                        iso: Some(vec![Source::Web(WebSource::new(url, checksum, None, None))]),
                                        ..Default::default()
                                    }
                                })
                                .collect::<Vec<Config>>(),
                        )
                    }
                })
                .collect::<Vec<_>>()
        });

        futures::future::join_all(futures)
            .await
            .into_iter()
            .flatten()
            .flatten()
            .collect::<Vec<Config>>()
            .into()
    }
}

const BAZZITE_WORKFLOW: &str = "https://raw.githubusercontent.com/ublue-os/bazzite/main/.github/workflows/build_iso.yml";
const BAZZITE_EXCLUDE: [&str; 3] = ["nvidia", "ally", "asus"];
const BAZZITE_MIRROR: &str = "https://download.bazzite.gg/";

pub struct Bazzite;
impl Distro for Bazzite {
    const NAME: &'static str = "bazzite";
    const PRETTY_NAME: &'static str = "Bazzite";
    const HOMEPAGE: Option<&'static str> = Some("https://bazzite.gg/");
    const DESCRIPTION: Option<&'static str> = Some("Container native gaming and a ready-to-game SteamOS like.");
    async fn generate_configs() -> Option<Vec<Config>> {
        let workflow = capture_page(BAZZITE_WORKFLOW).await?;
        let workflow_capture_regex = Regex::new(r#"- (bazzite-?(.*))"#).unwrap();

        let futures = workflow_capture_regex
            .captures_iter(&workflow)
            .map(|c| {
                let edition_capture = &c[2];

                let edition = if edition_capture.is_empty() {
                    "plasma".to_string()
                } else if edition_capture.len() > 4 {
                    edition_capture.to_string()
                } else {
                    format!("{edition_capture}-plasma")
                };

                let iso = format!("{BAZZITE_MIRROR}{}-stable.iso", &c[1]);
                async move {
                    if BAZZITE_EXCLUDE.iter().any(|e| edition.contains(e)) {
                        return None;
                    }
                    let checksum_url = iso.clone() + "-CHECKSUM";
                    let checksum = capture_page(&checksum_url)
                        .await
                        .and_then(|c| c.split_whitespace().next().map(ToString::to_string));
                    Some(Config {
                        release: Some("latest".to_string()),
                        edition: Some(edition),
                        iso: Some(vec![Source::Web(WebSource::new(iso, checksum, None, None))]),
                        ..Default::default()
                    })
                }
            })
            .collect::<Vec<_>>();

        futures::future::join_all(futures)
            .await
            .into_iter()
            .flatten()
            .collect::<Vec<Config>>()
            .into()
    }
}

const CENTOS_MIRROR: &str = "https://linuxsoft.cern.ch/centos-stream/";
const CENTOS_URL_PREFIX: &str = "https://mirrors.centos.org/mirrorlist?path=/";
const CENTOS_URL_SUFFIX: &str = "&redirect=1&protocol=https";

pub struct CentOSStream;
impl Distro for CentOSStream {
    const NAME: &'static str = "centos-stream";
    const PRETTY_NAME: &'static str = "CentOS Stream";
    const HOMEPAGE: Option<&'static str> = Some("https://www.centos.org/centos-stream/");
    const DESCRIPTION: Option<&'static str> =
        Some("Continuously delivered distro that tracks just ahead of Red Hat Enterprise Linux (RHEL) development, positioned as a midstream between Fedora Linux and RHEL.");
    async fn generate_configs() -> Option<Vec<Config>> {
        let releases = capture_page(CENTOS_MIRROR).await?;
        let release_regex = Regex::new(r#"href="([0-9]+)-stream/""#).unwrap();
        let iso_regex = Arc::new(Regex::new(r#"href="(CentOS-Stream-[0-9]+-[0-9]{8}.0-[^-]+-([^-]+)\.iso)""#).unwrap());

        let futures = release_regex
            .captures_iter(&releases)
            .flat_map(|c| {
                let release = c[1].to_string();
                [Arch::x86_64, Arch::aarch64]
                    .iter()
                    .map(|arch| {
                        let release = release.clone();
                        let iso_regex = iso_regex.clone();
                        let mirror_addition = format!("{release}-stream/BaseOS/{arch}/iso/");
                        let mirror = format!("{CENTOS_MIRROR}{mirror_addition}");
                        let final_mirror = format!("{CENTOS_URL_PREFIX}{mirror_addition}");
                        let checksum_url = mirror.clone() + "SHA256SUM";

                        async move {
                            let page = capture_page(&mirror).await?;
                            let mut checksums = ChecksumSeparation::Sha256Regex.build(&checksum_url).await;
                            Some(
                                iso_regex
                                    .captures_iter(&page)
                                    .map(|c| {
                                        let iso = &c[1];
                                        let url = format!("{final_mirror}{iso}{CENTOS_URL_SUFFIX}");
                                        let checksum = checksums.as_mut().and_then(|cs| cs.remove(iso));
                                        let edition = c[2].to_string();
                                        Config {
                                            release: Some(release.clone()),
                                            edition: Some(edition),
                                            arch: arch.clone(),
                                            iso: Some(vec![Source::Web(WebSource::new(url, checksum, None, None))]),
                                            ..Default::default()
                                        }
                                    })
                                    .collect::<Vec<Config>>(),
                            )
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        futures::future::join_all(futures)
            .await
            .into_iter()
            .flatten()
            .flatten()
            .collect::<Vec<Config>>()
            .into()
    }
}

const FEDORA_RELEASE_URL: &str = "https://fedoraproject.org/releases.json";
const VALID_FEDORA_FILETYPES: [&str; 2] = ["raw.xz", "iso"];
const BLACKLISTED_EDITIONS: [&str; 2] = ["Server", "Cloud_Base"];

pub struct Fedora;
impl Distro for Fedora {
    const NAME: &'static str = "fedora";
    const PRETTY_NAME: &'static str = "Fedora";
    const HOMEPAGE: Option<&'static str> = Some("https://fedoraproject.org/");
    const DESCRIPTION: Option<&'static str> = Some("Innovative platform for hardware, clouds, and containers, built with love by you.");
    async fn generate_configs() -> Option<Vec<Config>> {
        let mut releases = FedoraRelease::gather_data(FEDORA_RELEASE_URL).await?;
        // Filter out unwanted filetypes and editions
        releases.retain(|FedoraRelease { link, edition, .. }| VALID_FEDORA_FILETYPES.iter().any(|ext| link.ends_with(ext)) && !BLACKLISTED_EDITIONS.iter().any(|e| edition == e));

        releases
            .iter_mut()
            .for_each(|FedoraRelease { link, edition, archive_format, .. }| {
                if link.ends_with("raw.xz") {
                    *edition += "_preinstalled";
                    *archive_format = Some(ArchiveFormat::Xz);
                }
            });
        releases.dedup_by(|a, b| a.release == b.release && a.edition == b.edition);

        releases
            .into_iter()
            .filter_map(
                |FedoraRelease {
                     release,
                     edition,
                     arch,
                     link,
                     archive_format,
                     sha256,
                 }| {
                    let is_disk_image = archive_format.is_some();
                    let source = Source::Web(WebSource::new(link, sha256, archive_format, None));
                    let arch = arch_from_str(&arch)?;
                    let mut config = Config {
                        release: Some(release),
                        edition: Some(edition),
                        arch,
                        ..Default::default()
                    };
                    if is_disk_image {
                        config.disk_images = Some(vec![Disk {
                            source,
                            format: DiskFormat::Raw,
                            ..Default::default()
                        }])
                    } else {
                        config.iso = Some(vec![source]);
                    }
                    Some(config)
                },
            )
            .collect::<Vec<Config>>()
            .into()
    }
}
