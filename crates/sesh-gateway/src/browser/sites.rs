//! Which site a page is on, for `login(site)` (specs/010 research.md R5):
//! a credential saved for `example.org` is filled only on `example.org`
//! or one of its subdomains, compared label by label, so lookalikes
//! (`example.org.evil.test`, `examp1e.org`, `evilexample.org`) never get it.

/// A site name as the operator saves it: a lower-case DNS name, no
/// scheme, port, path or trailing dot.
pub fn valid_site(site: &str) -> bool {
    !site.is_empty()
        && site.len() <= 253
        && site.contains('.')
        && site.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

/// Whether a page at `url` belongs to `site`: an https (or, in stub mode,
/// http) address whose host is `site` or a subdomain of it.
pub fn on_site(url: &str, site: &str, allow_http: bool) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    let scheme_ok = u.scheme() == "https" || (allow_http && u.scheme() == "http");
    let Some(host) = u.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    scheme_ok && valid_site(site) && (host == site || host.ends_with(&format!(".{site}")))
}

/// The credential name an owner's login for a site is stored under
/// (`browser` connector, data-model.md "Site login"). Credential names are
/// `[A-Z0-9_]{1,64}`, so the owner and site are hashed into it; the
/// `browser_sites` table keeps the readable pair.
pub fn credential_name(owner: &str, site: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(format!("{owner}\0{site}").as_bytes());
    let hex: String = h.iter().take(16).map(|b| format!("{b:02X}")).collect();
    format!("SITE_{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_site_and_its_subdomains_match() {
        for u in [
            "https://example.org/login",
            "https://www.example.org/",
            "https://login.eu.example.org/a?b=c",
            "https://EXAMPLE.org./x",
        ] {
            assert!(on_site(u, "example.org", false), "{u}");
        }
    }

    #[test]
    fn lookalikes_and_other_schemes_do_not() {
        for u in [
            "https://example.org.evil.test/",
            "https://evilexample.org/",
            "https://examp1e.org/",
            "https://example.com/",
            "https://org/",
            "http://example.org/",
            "https://user@evil.test/example.org",
            "javascript:alert(1)",
            "file:///example.org",
            "not a url",
        ] {
            assert!(!on_site(u, "example.org", false), "{u}");
        }
        assert!(
            on_site("http://fake-bank.test:8000/", "fake-bank.test", true),
            "stub mode"
        );
        assert!(!on_site(
            "http://fake-bank.test.evil.test/",
            "fake-bank.test",
            true
        ));
    }

    #[test]
    fn site_names_are_plain_dns_names() {
        assert!(valid_site("example.org") && valid_site("fake-bank.test"));
        for bad in [
            "",
            "org",
            "Example.org",
            "example.org/",
            "https://example.org",
            "a..b",
            "-a.org",
            "a b.org",
        ] {
            assert!(!valid_site(bad), "{bad:?}");
        }
        let n = credential_name("ana", "example.org");
        assert!(sesh_core::valid_credential_name(&n), "{n}");
        assert_ne!(n, credential_name("ben", "example.org"), "per owner");
        assert_ne!(n, credential_name("ana", "example.com"), "per site");
        assert_eq!(n, credential_name("ana", "example.org"), "stable");
    }
}
