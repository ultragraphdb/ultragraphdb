## ADDED Requirements

### Requirement: Operations run through an admin interface
The Alpha SHALL offer export, backup, restore, listing backups and moving a tablet through an admin interface open to
the superadmin alone, each returning a task identifier whose status can be queried.

#### Scenario: A superadmin starts an export and follows it
- **WHEN** the superadmin starts an export and queries its task
- **THEN** the task reports queued, running and then success with the export's location

#### Scenario: A guardian of another namespace cannot start a backup
- **WHEN** a guardian of namespace 2 starts a backup
- **THEN** it is refused with `PERMISSION_DENIED`
