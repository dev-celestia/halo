//! Static copy for CA install / troubleshooting guides.

pub struct Guide {
    pub title: &'static str,
    pub steps: &'static [&'static str],
    pub note: Option<&'static str>,
}

pub const INSTALLATION_GUIDES: &[Guide] = &[
    Guide {
        title: "Chrome / Edge (Windows)",
        steps: &[
            "Open Chrome and go to Settings → Privacy and security",
            "Click \"Manage certificates\"",
            "Go to the \"Authorities\" tab",
            "Click \"Import\" and select the saved halo-ca.pem file",
            "When prompted, check \"Trust this certificate for identification of websites\"",
            "Click \"OK\" and restart your browser",
        ],
        note: None,
    },
    Guide {
        title: "Chrome / Edge (macOS)",
        steps: &[
            "Open Chrome and go to Settings → Privacy and security → Security",
            "Scroll down and click \"Manage certificates\"",
            "Click \"Import\" in the dialog that appears",
            "Select the saved halo-ca.pem file",
            "Check \"Trust for SSL/TLS websites\" when prompted",
            "Click \"OK\" and restart your browser",
        ],
        note: None,
    },
    Guide {
        title: "Firefox (All Platforms)",
        steps: &[
            "Open Firefox and go to Options → Privacy & Security",
            "Scroll to \"Certificates\" and click \"View Certificates\"",
            "Click the \"Authorities\" tab",
            "Click \"Import\" and select the saved halo-ca.pem file",
            "Check \"Trust this CA to identify websites\"",
            "Click \"OK\" and restart Firefox",
        ],
        note: Some("Firefox uses its own certificate store and ignores the system keychain."),
    },
    Guide {
        title: "Safari (macOS)",
        steps: &[
            "Prefer \"Install to macOS Keychain\" above — Safari trusts the login keychain",
            "Or open Keychain Access and import halo-ca.pem manually",
            "Set certificate trust to \"Always Trust\" for SSL",
            "Restart Safari",
        ],
        note: None,
    },
];

pub const HOW_IT_WORKS: &[(&str, &str)] = &[
    (
        "1. Certificate Generation",
        "On first launch Halo generates a unique Root CA stored locally on this device.",
    ),
    (
        "2. CA Installation",
        "Installing this CA tells your browser or OS to trust certificates signed by Halo.",
    ),
    (
        "3. Dynamic Certificate Signing",
        "When you visit an HTTPS site, the proxy mints a leaf certificate for that host, signed by the local CA.",
    ),
    (
        "4. Secure Passthrough",
        "The proxy decrypts, inspects, and re-encrypts traffic. Your browser sees a valid chain and the padlock.",
    ),
    (
        "5. Privacy Boundary",
        "Only traffic routed through the configured proxy ports is intercepted.",
    ),
];

pub const TROUBLESHOOTING: &[(&str, &str)] = &[
    (
        "Untrusted certificate warnings",
        "Re-install the CA, then fully quit and reopen the browser. Confirm the PEM was imported into the Authorities / System Roots store.",
    ),
    (
        "Mobile apps / certificate pinning",
        "Pinned apps reject MITM certificates by design. Use the managed Open Browser profile or disable pinning in a lab build.",
    ),
    (
        "Removing the CA",
        "macOS: Keychain Access → search \"Halo Proxy CA\" (legacy installs: \"Hexbuffer Proxy CA\") → delete. Linux/Chrome: certutil -D -n \"Halo Proxy CA\" -d sql:~/.pki/nssdb",
    ),
];
