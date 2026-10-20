## ADDED Requirements

### Requirement: Operations run through an admin interface
The Alpha SHALL offer export, backup, restore, listing backups and moving a tablet through an admin interface open to
the superadmin alone; export, backup and restore SHALL return a task identifier whose status is queued, running,
success or failed. Dgraph queues only backup and export, answers restore and tablet moves synchronously, and lets a
namespace's guardian export that namespace; the port confines export to the superadmin.

#### Scenario: A superadmin starts an export and follows it
- **WHEN** the superadmin starts an export and polls its task until it ends
- **THEN** the statuses reported are a subsequence of queued, running and success, ending in success

#### Scenario: A guardian of another namespace cannot start a backup
- **WHEN** a guardian of namespace 2 starts a backup
- **THEN** it is refused as permission denied
