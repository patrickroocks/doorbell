#include "configdiagnosticsdialog.h"
#include "ui_configdiagnosticsdialog.h"

#include "configstore.h"
#include "constants.h"
#include "ringlistener.h"

#include <QMessageBox>
#include <QScreen>

namespace
{
const int BorderSize = 5;
const QString WidgetBorderStyle = "#BorderWdg {border: 1px solid #C2C2C2;}";
}

CategoryListModel::CategoryListModel(const std::vector<Category>& categories)
	: categories(categories)
{}

int CategoryListModel::rowCount(const QModelIndex& parent) const
{
	Q_UNUSED(parent);

	return categories.size();
}

QVariant CategoryListModel::data(const QModelIndex& index, int role) const
{
	if (!index.isValid() || index.row() < 0 || index.row() >= categories.size()) return QVariant();

	if (role == Qt::DisplayRole) {
		return categories[index.row()].name;
	}

	return QVariant();
}

// ---------------------------------------------------------------------------------------------------------

ConfigDiagnosticsDialog::ConfigDiagnosticsDialog(RingListener* ringListener, ConfigStore* cfgStore)
	: CommandClientDialog(ringListener)
	, cfgStore(cfgStore)
	, ringListener(ringListener)
	, ui(new Ui::ConfigDiagnosticsDialog)
{
	ui->setupUi(this);

	loadSettings(cfgStore->getCurrentConfig());
	subscribeToState();
	initializeCategories();
	establishUiConnections();
}

void ConfigDiagnosticsDialog::establishUiConnections()
{
	const auto* applyButton = ui->buttonBox->button(QDialogButtonBox::Apply);
	const auto* restoreButton = ui->buttonBox->button(QDialogButtonBox::RestoreDefaults);

	connect(applyButton, &QPushButton::clicked, this, &ConfigDiagnosticsDialog::applySettings);
	connect(restoreButton, &QPushButton::clicked, this, &ConfigDiagnosticsDialog::restoreDefaults);

	connect(ui->cmdUpdateRawData, &QPushButton::clicked, this, &ConfigDiagnosticsDialog::updateRawData);
	connect(ui->cmdUpdateHistory, &QPushButton::clicked, this, &ConfigDiagnosticsDialog::updateActionLog);
	connect(ui->cmdUpdateStartTime, &QPushButton::clicked, this, &ConfigDiagnosticsDialog::updateStartTime);
	connect(ui->cmdChangeAutoBuzz, &QPushButton::clicked, this, &ConfigDiagnosticsDialog::changeAutoBuzz);
	connect(ui->cmdTestBuzzer, &QPushButton::clicked, this, &ConfigDiagnosticsDialog::testBuzz);
	connect(ui->cmdTestRing, &QPushButton::clicked, this, &ConfigDiagnosticsDialog::testRing);
	connect(ui->cmdReconnectNow, &QPushButton::clicked, ringListener, &RingListener::reconnect);
}

void ConfigDiagnosticsDialog::restoreDefaults() { loadSettings(cfgStore->getDefaultConfig()); }

void ConfigDiagnosticsDialog::subscribeToState()
{
	ui->lblConnState->setText(ringListener->getConnStateStr());
	connect(ringListener, &RingListener::connStateChanged, ui->lblConnState, &QLabel::setText);

	ui->lblAutoBuzzerState->setText(ringListener->getAutoBuzzStateStr());
	connect(ringListener, &RingListener::autoBuzzStateChanged, ui->lblAutoBuzzerState, &QLabel::setText);
}

void ConfigDiagnosticsDialog::placeCenter()
{
	QScreen* screen = QGuiApplication::primaryScreen();
	QRect screenGeometry = screen->geometry();
	// Place in the center:
	const int x = (screenGeometry.width() - this->width()) / 2;
	const int y = (screenGeometry.height() - this->height()) / 2;
	this->setGeometry(x, y, this->width(), this->height());
}

void ConfigDiagnosticsDialog::showEvent(QShowEvent* event) { placeCenter(); }

void ConfigDiagnosticsDialog::initializeCategories()
{
	categories.push_back({"Main State", ui->wdgMainState});
	categories.push_back({"Settings", ui->wdgSettings});
	categories.push_back({"Ring/Buzz log", ui->wdgActionLog});
	categories.push_back({"Raw data", ui->wdgRawData});
	categories.push_back({"About", ui->wdgAbout});

	categoryListModel = QSharedPointer<CategoryListModel>::create(categories);
	ui->lstCategory->setModel(categoryListModel.get());
	connect(ui->lstCategory->selectionModel(), &QItemSelectionModel::selectionChanged, this, &ConfigDiagnosticsDialog::onCategoryChange);

	prepareAboutWidget();

	// Visibility of all widgets is initially false:
	for (auto& category : categories) category.widget->setVisible(false);

	const auto primaryWidget = categories.at(0).widget;

	// Place categories left
	ui->lstCategory->setGeometry(BorderSize, BorderSize, ui->lstCategory->geometry().width(), primaryWidget->geometry().height());

	// Place primary widget right to the button box
	primaryWidget->setGeometry(ui->lstCategory->geometry().right() + BorderSize,
							   BorderSize,
							   primaryWidget->geometry().width(),
							   primaryWidget->geometry().height());

	// Place ButtonBox at the bottom of the dialog, spaning over categories and primary widget
	ui->buttonBox->setGeometry(BorderSize,
							   primaryWidget->geometry().height() + 2 * BorderSize,
							   primaryWidget->geometry().width() + BorderSize + ui->lstCategory->geometry().width(),
							   ui->buttonBox->geometry().height());

	// Size of the dialog is adjusted to button box
	this->setGeometry(this->geometry().left(),
					  this->geometry().top(),
					  ui->buttonBox->geometry().right() + BorderSize,
					  ui->buttonBox->geometry().bottom() + BorderSize);

	// Place all the widges equally to the primary widget
	for (auto& category : categories)
	{
		category.widget->setGeometry(primaryWidget->geometry());
		category.widget->setObjectName("BorderWdg");
		category.widget->setStyleSheet(WidgetBorderStyle);
	}

	// Do not allow to resize the dialog:
	this->setFixedSize(this->size());

	// Initially category 0 shall be selected
	ui->lstCategory->setCurrentIndex(categoryListModel->index(0));
}

ConfigDiagnosticsDialog::~ConfigDiagnosticsDialog() { delete ui; }

void ConfigDiagnosticsDialog::prepareAboutWidget()
{
	util::setPictureInLabel(":/img/logo.png", ui->lblLogo);
	ui->lblAbout->setText(ui->lblAbout->text().arg(Version));
	// height of about label should be equal to height of logo
	ui->lblAbout->setGeometry(ui->lblAbout->x(), ui->lblLogo->y(), ui->lblAbout->width(), ui->lblLogo->height());
}

void ConfigDiagnosticsDialog::onCategoryChange(const QItemSelection& selected, const QItemSelection& deselected)
{
	if (!deselected.isEmpty())
	{
		auto index = deselected.indexes().first();
		categories[index.row()].widget->setVisible(false);
	}

	if (!selected.isEmpty())
	{
		auto index = selected.indexes().first();
		auto& selectedCategory = categories[index.row()];
		selectedCategory.widget->setVisible(true);
		updateSelectedCategory();
	}
}

void ConfigDiagnosticsDialog::updateSelectedCategory()
{
	auto& selectedCategory = categories[ui->lstCategory->currentIndex().row()];

	if (selectedCategory.widget == ui->wdgMainState)
		updateStartTime();
	else if (selectedCategory.widget == ui->wdgActionLog)
		updateActionLog();
	else if (selectedCategory.widget == ui->wdgRawData)
		updateRawData();
}

void ConfigDiagnosticsDialog::updateRawData()
{
	ui->txtRawData->setText("Receiving raw data...");
	sendCommand(Command::getRawData);
}

void ConfigDiagnosticsDialog::changeAutoBuzz()
{
	sendCommand(ringListener->getAutoBuzzState() ? Command::autoBuzzOff : Command::autoBuzzOn);
}

void ConfigDiagnosticsDialog::updateActionLog()
{
	ui->txtActionLog->setText("Receiving action log...");
	sendCommand(Command::getActionLog);
}

void ConfigDiagnosticsDialog::updateStartTime()
{
	ui->lblDeviceStartTime->setText("Receiving start time...");
	sendCommand(Command::getStartTime);
}

void ConfigDiagnosticsDialog::accept()
{
	applySettings();
	QDialog::accept();
}

void ConfigDiagnosticsDialog::applySettings()
{
	cfgStore->setBrokerAddr(ui->txtBrokerAddr->text());
	cfgStore->setWlanPattern(ui->txtWlanPattern->text());
	ringListener->reloadSettings();
}

void ConfigDiagnosticsDialog::onReceiveCommandResponse(const Command& cmd, const QByteArray& response)
{
	if (cmd == Command::getStartTime)
	{
		ui->lblDeviceStartTime->setText(QString::fromUtf8(response));
	}
	else if (cmd == Command::getRawData)
	{
		ui->txtRawData->setText(response.isEmpty() ? "(empty)" : QString::fromUtf8(response));
	}
	else if (cmd == Command::getActionLog)
	{
		ui->txtActionLog->setText(response.isEmpty() ? "(empty)" : QString::fromUtf8(response));
	}
}

void ConfigDiagnosticsDialog::loadSettings(const Config& config)
{
	ui->txtBrokerAddr->setText(config.brokerAddr);
	ui->txtWlanPattern->setText(config.wlanPattern);
}

void ConfigDiagnosticsDialog::testRing() { sendCommand(Command::testRing); }

void ConfigDiagnosticsDialog::testBuzz()
{
	QMessageBox::StandardButton reply;
	reply = QMessageBox::question(this, "Test Buzzer", "Do you really want to press the buzzer?", QMessageBox::Yes | QMessageBox::No);
	if (reply == QMessageBox::Yes)
		sendCommand(Command::buzz);
}
