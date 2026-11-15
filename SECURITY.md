# Security policy

If you find a security problem in Neon Strider, please report it privately. Do not open a
public issue.

Use GitHub's **Report a vulnerability** button on the repository's **Security** tab (private
security advisories). Include what you found, how to reproduce it, and the version you tested.

I'll acknowledge reports as soon as I can and keep you updated while a fix is prepared.

Neon Strider keeps its Strava client secret and tokens in Windows Credential Manager. Anything
that exposes those, or that lets another program control the treadmill without the user's
knowledge, is in scope.
