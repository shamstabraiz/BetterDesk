# Lessons

- When user redirects focus to Discussions, pause issue triage and respond directly in the active discussion with concrete diagnostic steps.
- For i18n completeness checks in this repo, use pl.json as the canonical comparison baseline when the user indicates it is the most complete locale; do not assume en.json is the source of truth.
- Before declaring i18n completeness in this repo, inventory every active translation system separately: web-nodejs/lang for the web console, yomie-client/src/locales for the desktop client, and exclude archive/installer language channels unless the user explicitly asks to include them.
- For locale audits in this repo, treat missing=0 as structural completeness only; when the user still reports large gaps, scan non-reference locales for raw English fallback values because keys can exist while visible UI text is still untranslated.
