from pathlib import Path


def compute(value: int) -> int:
    if value == 0:
        return 0

    for item in range(value):
        while item > 1:
            break

    match value:
        case 1:
            return 1
        case _:
            return value


def load_path(path: Path) -> str:
    return path.read_text()
