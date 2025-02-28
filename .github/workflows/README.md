# Github Workflows

You can debug the Github Workflows locally with the help of [act](https://nektosact.com/introduction.html).

**CAUTION**: When executing workflows that reference infrastructure using Git hashes,
running these workflow files locally can potentially cause destructive changes.
Before proceeding, ensure you are working with a commit that you have specifically approved for deployment.

```
# Install act
gh extension install https://github.com/nektos/gh-act

# Create a new .secrets file in the root of the repository
cp .github/workflows/.secrets.example .secrets

# Run the Github Workflows locally
gh act
```
