"""A minimal Python package for integration testing."""

__version__ = '1.0.0'


def greet(name):
    """
    Returns a greeting message.

    Args:
        name (str): The name to greet

    Returns:
        str: A greeting string
    """
    return f"Hello, {name}!"


def get_version():
    """
    Returns the package version.

    Returns:
        str: Version string
    """
    return __version__
