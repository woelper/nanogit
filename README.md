# NanoGit is (and will remain) an absolutely minimal graphical git client and library.

## The world does not need another one of these, does it?
That is perhaps true. There are however scenarios where this could have a benefit: NanoGit is designed as a companion app that supports tools that don't have adequate git support and need a minimal subset.

### Features
- Stage, unstage, commit. See a diff.


### Library
- git-like functionality. This library is based on `git2`, but dramatically simplifies it if you just want to get a status or list of branches / commits. In general, the library tries to stay close to the git command line in terms of functionality and terminology. Git2 and gix are much harder to use.
- Cached Repository: Costly operations are cached and can be accessed later at no speed cost.
