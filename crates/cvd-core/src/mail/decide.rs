pub const AUTO_APPLY_THRESHOLD: f64 = 0.85;
pub const JOB_RELATED_THRESHOLD: f64 = 0.5;

#[derive(Debug, Clone, PartialEq)]
pub struct Classified {
    pub job_related: f64,
    pub matched_job: Option<i64>,
    pub status_changed: f64,
    pub new_status: Option<(i64, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Ignored,
    NoChange {
        job_id: i64,
    },
    Applied {
        job_id: i64,
        status_id: i64,
        confidence: f64,
    },
    PendingReview {
        job_id: i64,
        status_id: i64,
        confidence: f64,
    },
}

pub fn decide(c: &Classified, current_status: Option<i64>) -> Decision {
    if c.job_related < JOB_RELATED_THRESHOLD {
        return Decision::Ignored;
    }

    let Some(job_id) = c.matched_job else {
        return Decision::Ignored;
    };

    if c.status_changed < JOB_RELATED_THRESHOLD {
        return Decision::NoChange { job_id };
    }

    let Some((status_id, confidence)) = c.new_status else {
        return Decision::NoChange { job_id };
    };

    if Some(status_id) == current_status {
        return Decision::NoChange { job_id };
    }

    if c.status_changed >= AUTO_APPLY_THRESHOLD && confidence >= AUTO_APPLY_THRESHOLD {
        Decision::Applied {
            job_id,
            status_id,
            confidence,
        }
    } else {
        Decision::PendingReview {
            job_id,
            status_id,
            confidence,
        }
    }
}
