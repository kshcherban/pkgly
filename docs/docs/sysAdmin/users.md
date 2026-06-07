# User Administration

Administrators and user managers can create local users from **Administration > Users**.
The create form submits the user's identity, password, elevated roles, and default repository
permissions together. The user is only created when the complete operation succeeds.

## Initial Permissions

The administration interface creates users with these defaults:

- Admin: disabled
- User Manager: disabled
- System Manager: disabled
- Default repository Read: enabled
- Default repository Write: disabled
- Default repository Edit: disabled

Default repository permissions apply when the user has no repository-specific override. Configure
repository-specific permissions from the user's permission screen after creation.

API clients may include a `permissions` object in `POST /api/user-management/create`:

```json
{
  "name": "Example User",
  "username": "example",
  "email": "example@example.com",
  "password": "replace-with-a-secure-password",
  "permissions": {
    "admin": false,
    "user_manager": false,
    "system_manager": false,
    "default_repository_actions": ["Read"]
  }
}
```

When an API client omits `permissions`, the user receives no elevated roles and no default
repository access. The Read default is applied by the administration interface, not the API.
