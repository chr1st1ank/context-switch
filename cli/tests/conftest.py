"""Shared test fixtures."""

import pytest


@pytest.fixture
def sample_project() -> str:
    """A sample project name for testing."""
    return "test-project"
