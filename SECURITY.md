# Security Policy

## Supported Versions

Only the latest stable release is supported by the `bumpalo` maintainers and
they will only issue formal security advisories for that version.

Pull requests sending fixes for bugs (security-related or otherwise) in older
major version branches may be accepted at the `bumpalo` maintainers' discretion,
but the maintainers will not issue formal security advisories for them.

## Reporting a Vulnerability

Use the following link to report security vulnerabilities in `bumpalo`:

<div align="center">

### *[Report a security vulnerability in `bumpalo`](https://github.com/fitzgen/bumpalo/security/advisories/new)*

</div>

Reports should include:

* **Summmary:** A concise (1-3 sentence) summary of the issue and its impact.

* **Details:** The nitty-gritty details of the issue, including the sequence of
  events required and links to relevant methods in the source.

* **Proof-of-Concept Reproducer:** A minimal program or `#[test]` that
  demonstrates the security vulnerability using `bupmalo`'s public API. Ideally
  this fails when run under MIRI, raises a segfault, or triggers an assertion
  failure inside of `bumpalo`. Include instructions on exactly how to compile and
  run this program to reproduce the vulnerability.

* **Impact:** What happens when the vulnerability is triggered? Use-after-free bug,
  arbitrary heap reads or writes, etc... What methods are affected?

* **Platform and Version Info:** What ISA (x86-64, aarch64, etc...), operating
  system, and operating system version are you reproducing on? What version or
  commit of `bumpalo`?
