# Developer Certificate of Origin and sign-off policy

Contributions to eutheto use Developer Certificate of Origin (DCO) 1.1 sign-off. The project does not require a Contributor License Agreement (CLA).

## Required workflow

Every commit submitted for inclusion must contain a sign-off trailer added by the contributor who is making the certification:

```text
Signed-off-by: Your Name <your.email@example.com>
```

Add the trailer with Git rather than typing it into the commit body:

```sh
git commit --signoff
```

The name and email in the trailer must identify the person making the certification and must be consistent with that commit's author identity. Signing off is a certification under the exact DCO text below; it is not merely an acknowledgement of this policy. Do not add a sign-off for another person unless you are authorized to certify the contribution on their behalf under the DCO.

Before submitting, inspect every commit in the proposed contribution. To repair the most recent commit, amend it and add the trailer:

```sh
git commit --amend --signoff
```

If several commits lack sign-off, rewrite each affected commit (for example, with an interactive rebase) and add its own `Signed-off-by` trailer. A pull-request description, a sign-off on only the final commit, or a co-author trailer does not certify the other commits. After rewriting commits, update the proposed branch through the hosting service's normal safe force-update workflow.

Multiple contributors may add separate trailers when each is making the certification. Automated commits must be attributable to an accountable person or approved automation identity that has authority to make the certification; automation does not waive DCO enforcement.

A contribution with a missing, malformed, or unauthorized sign-off cannot be merged. The contributor must correct the commit history; maintainers do not fabricate or silently add certifications. The repository's DCO check is the merge-time enforcement authority once that check exists.

## Developer Certificate of Origin 1.1

The following certification is reproduced verbatim from the Developer Certificate of Origin, Version 1.1.

```text
Developer Certificate of Origin
Version 1.1

Copyright (C) 2004, 2006 The Linux Foundation and its contributors.

Everyone is permitted to copy and distribute verbatim copies of this
license document, but changing it is not allowed.


Developer's Certificate of Origin 1.1

By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I
    have the right to submit it under the open source license
    indicated in the file; or

(b) The contribution is based upon previous work that, to the best
    of my knowledge, is covered under an appropriate open source
    license and I have the right under that license to submit that
    work with modifications, whether created in whole or in part
    by me, under the same open source license (unless I am
    permitted to submit under a different license), as indicated
    in the file; or

(c) The contribution was provided directly to me by some other
    person who certified (a), (b) or (c) and I have not modified
    it.

(d) I understand and agree that this project and the contribution
    are public and that a record of the contribution (including all
    personal information I submit with it, including my sign-off) is
    maintained indefinitely and may be redistributed consistent with
    this project or the open source license(s) involved.
```
