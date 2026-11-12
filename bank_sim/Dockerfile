FROM python:3.13-slim

ENV PYTHONUNBUFFERED=1
WORKDIR /srv/bank
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY app.py .
EXPOSE 5001
CMD ["python", "app.py"]
