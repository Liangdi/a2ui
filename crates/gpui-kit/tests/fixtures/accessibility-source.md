# Accessibility fixture provenance

`accessibility.json` is a lossless JSON conversion of `conformance/core/accessibility.yaml`
from the local official a2ui-project/a2ui specification checkout (Apache-2.0).
Contains seven cases, including the legacy unprefixed fallback case.

Tests against this fixture exercise metadata resolution and AccessKit node properties,
not a running platform accessibility tree. They do not establish axe-core results,
focus behavior under hidden ancestors, or VoiceOver announcements. The styled Input
API lacks textbox description metadata; ChoicePicker currently uses a native select
rather than the required mutually-exclusive radiogroup. These remain native gaps.

Pinned source commit: `eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41`.
Original copyright: Copyright 2024 Google LLC; Apache License, Version 2.0.
Source: https://github.com/a2ui-project/a2ui/blob/eb1fd7f0f8e9ff78d29ab6da7e3169bc1edb0f41/conformance/core/accessibility.yaml
