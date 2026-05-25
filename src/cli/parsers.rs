pub(crate) fn parse_usize_at_least(raw: &str, label: &str, min: usize) -> Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|error| format!("invalid {label}: {error}"))?;
    if value < min {
        Err(format!("{label} must be at least {min}"))
    } else {
        Ok(value)
    }
}

pub(crate) fn parse_usize_at_most(raw: &str, label: &str, max: usize) -> Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|error| format!("invalid {label}: {error}"))?;
    if value > max {
        Err(format!("{label} must be at most {max}"))
    } else {
        Ok(value)
    }
}

pub(crate) fn parse_usize_between(
    raw: &str,
    label: &str,
    min: usize,
    max: usize,
) -> Result<usize, String> {
    let value = parse_usize_at_least(raw, label, min)?;
    if value > max {
        Err(format!("{label} must be at most {max}"))
    } else {
        Ok(value)
    }
}

pub(crate) fn parse_u32_at_most(raw: &str, label: &str, max: u32) -> Result<u32, String> {
    let value = raw
        .parse::<u32>()
        .map_err(|error| format!("invalid {label}: {error}"))?;
    if value > max {
        Err(format!("{label} must be at most {max}"))
    } else {
        Ok(value)
    }
}
