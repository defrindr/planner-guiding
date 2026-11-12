use super::{ok, shared::report_date, ShopCore};
use crate::http::{ApiError, ApiRequest, ApiResponse};
use crate::repository::reports::{self, ReportGroup};
use axum::http::StatusCode;
use flate2::{write::GzEncoder, Compression};
use serde_json::json;
use std::{fs, io::Write, path::Path};

impl ShopCore {
    pub(crate) async fn export(
        &self,
        filename: &str,
        from: &str,
        to: &str,
    ) -> Result<String, ApiError> {
        let from_date = report_date(from)?;
        let to_date = report_date(to)?;
        let safe = Path::new(filename)
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("sales.csv");
        fs::create_dir_all(&self.context.config.export_dir)
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "export error"))?;
        let rows = reports::export_rows(&self.context.pool, from_date, to_date)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        let mut s = "order_no,date,status,user,product,qty,price\n".to_string();
        for r in rows {
            s.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                r.order_no, r.created_at, r.status, r.username, r.product, r.quantity, r.price
            ));
        }
        let p = self.context.config.export_dir.join(safe);
        fs::write(&p, &s)
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "export error"))?;
        let gz = Path::new(&format!("{}.gz", p.display())).to_path_buf();
        let mut z = GzEncoder::new(Vec::new(), Compression::default());
        z.write_all(s.as_bytes())
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "export error"))?;
        fs::write(
            &gz,
            z.finish()
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "export error"))?,
        )
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "export error"))?;
        Ok(gz.file_name().unwrap().to_string_lossy().into())
    }

    pub(crate) async fn report(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let _ = self.admin(&req).await?;
        let from = req
            .query
            .get("from")
            .map(String::as_str)
            .unwrap_or("1970-01-01");
        let to = req
            .query
            .get("to")
            .map(String::as_str)
            .unwrap_or("2999-12-31");
        let g = req
            .query
            .get("group_by")
            .map(String::as_str)
            .unwrap_or("day");
        let group = match g {
            "status" => ReportGroup::Status,
            "user" => ReportGroup::User,
            _ => ReportGroup::Day,
        };
        let from_date = report_date(from)?;
        let to_date = report_date(to)?;
        let rows = reports::buckets(&self.context.pool, group, from_date, to_date)
            .await
            .map_err(|error| {
                tracing::error!(?error, "sales report grouping query failed");
                ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
            })?;
        let top_rows = reports::top_products(&self.context.pool, from_date, to_date)
            .await
            .map_err(|error| {
                tracing::error!(?error, "sales report top-products query failed");
                ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
            })?;
        let buckets = rows
            .into_iter()
            .map(|r| {
                json!({
                    "bucket": r.bucket,
                    "orders": r.orders,
                    "revenue": r.revenue
                })
            })
            .collect::<Vec<_>>();
        let top_products = top_rows
            .into_iter()
            .map(|r| json!({"name": r.name, "revenue": r.revenue}))
            .collect::<Vec<_>>();
        ok(json!({
            "from": from,
            "to": to,
            "group_by": g,
            "buckets": buckets,
            "top_products": top_products
        }))
    }
}
