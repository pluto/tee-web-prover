#!/bin/bash
set -ex

export HOME=/root

export PROJECT_NAME=$(curl -s "http://169.254.169.254/computeMetadata/v1/project/project-id" -H "Metadata-Flavor: Google")
export GIT_HASH=$(curl -s "http://169.254.169.254/computeMetadata/v1/instance/attributes/git-hash" -H "Metadata-Flavor: Google")
export GIT_BRANCH=$(curl -s "http://169.254.169.254/computeMetadata/v1/instance/attributes/git-branch" -H "Metadata-Flavor: Google")
export DOMAIN=$(curl -s "http://169.254.169.254/computeMetadata/v1/instance/attributes/domain" -H "Metadata-Flavor: Google")

mkdir /etc/tee
echo -n $GIT_HASH > /etc/tee/git_hash
echo -n $GIT_BRANCH > /etc/tee/git_branch
echo -n $DOMAIN > /etc/tee/domain

# Install global dependencies
apt update
apt install -y build-essential

echo "Installing Rust"
curl https://sh.rustup.rs -sSf | sh -s -- -y

# Create tee user
if ! id -u tee &>/dev/null; then
  useradd --system --shell /usr/sbin/nologin tee
else
  log "User 'tee' already exists"
fi

# Clone repo
mkdir /opt/tee
git clone --depth 1 --branch feat/github-tee-deploy-2 https://github.com/pluto/tee-web-prover.git /opt/tee/tee-web-prover
cd /opt/tee/tee-web-prover

# Run startup scripts
for startupscript in $(find .github/workflows/tee/services -name "startup-*.sh"); do
  echo "Processing $startupscript"
  sh $startupscript
done

chown -R tee:tee /opt/tee

# Enable and start services
for servicefile in $(find .github/workflows/tee/services -name "*.service"); do
  echo "Found service file: $servicefile"

  # Link service file to systemd directory
  ln -sf "$(pwd)/$servicefile" /etc/systemd/system/

  # Enable and start the service
  systemctl daemon-reload
  systemctl enable "$(basename "$servicefile")"
  systemctl start "$(basename "$servicefile")"
done
