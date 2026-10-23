## ADDED Requirements

### Requirement: A backup holds only valid UTF-8 strings
A backup SHALL check every string value it writes for valid UTF-8 before writing it, and SHALL fail, naming the key,
when one is not. The store refuses such values at mutation time, so the check guards the backup against a defect
upstream; a restore then reads only valid strings, and text analysis on restore takes them as `&str` (ledger 17, 18,
31).

#### Scenario: A backup refuses a string value that is not UTF-8
- **WHEN** a backup meets a string value whose bytes are not valid UTF-8
- **THEN** the backup fails with an error naming the value's key and publishes no manifest

#### Scenario: A backup of valid strings succeeds
- **WHEN** a predicate holding `İstanbul` and `日本` is backed up and restored
- **THEN** the restored values equal the originals
