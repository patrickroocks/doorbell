#pragma once

#include "commandclient.h"

#include <QAbstractListModel>
#include <qitemselectionmodel.h>

namespace Ui {
class ConfigDiagnosticsDialog;
}

class Config;
class ConfigStore;
class RingListener;

struct Category final
{
	QString name;
	QWidget* widget = nullptr;
};

class CategoryListModel final : public QAbstractListModel
{
	Q_OBJECT
public:
	CategoryListModel(const std::vector<Category>& categories);

	int rowCount(const QModelIndex& parent = QModelIndex()) const override;
	QVariant data(const QModelIndex& index, int role) const override;

private:
	const std::vector<Category>& categories;
};

// ---------------------------------------------------------------------------------------------------------

class ConfigDiagnosticsDialog final : public CommandClientDialog
{
	Q_OBJECT

public:
	explicit ConfigDiagnosticsDialog(RingListener* ringListener, ConfigStore* cfgStore);
	~ConfigDiagnosticsDialog();

	void updateSelectedCategory();

protected:
	void accept() override;

	void onReceiveCommandResponse(const Command& cmd, const QByteArray& response) override;
	void showEvent(QShowEvent* event) override;

private slots:
	void onCategoryChange(const QItemSelection& selected, const QItemSelection& deselected);

private:
	void prepareAboutWidget();
	void placeCenter();
	void loadSettings(const Config& config);
	void applySettings();
	void restoreDefaults();

	void testRing();
	void testBuzz();
	void changeAutoBuzz();

	void updateRawData();
	void updateStartTime();
	void updateActionLog();

	void establishUiConnections();
	void subscribeToState();
	void initializeCategories();

	ConfigStore* const cfgStore;
	RingListener* const ringListener;
	std::vector<Category> categories;
	QSharedPointer<CategoryListModel> categoryListModel;

	Ui::ConfigDiagnosticsDialog* ui;
};
