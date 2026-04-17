# Project Overview

This is the main documentation for the project.

## Architecture

The system uses a layered architecture.

See [setup guide](./docs/setup.md) for installation.
Also check [[Design Decisions]] for context.

### Components

- Auth module handles authentication
- API layer handles requests

```typescript
function authenticate(token: string): boolean {
  return validateToken(token);
}
```

## API Reference

### Endpoints

```bash
curl -X POST /api/auth
```

## Links

- [Contributing](./CONTRIBUTING.md)
- [License](https://opensource.org/licenses/MIT)
