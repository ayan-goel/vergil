# access-role-escalation-self-grant

Detects role-based access systems whose grant path doesn't enforce the admin-role hierarchy. Vergil's negation property requires that every grantRole call originate from a caller holding the role's admin role. The Halmos encoding routes a non-admin Attacker through grantRole and asserts the attacker does not end up holding the target role. See `manifest.yaml` and `notes/attack-patterns.md` §1.6.
