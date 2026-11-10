function Init()
	local m = NewWebsiteModule()
	m.ID                         = 'db60a87536a446178841c5024ddf111c'
	m.Name                       = 'Wnacg'
	m.RootURL                    = 'https://www.wnacg.org'
	m.Category                   = 'Raw'
	m.OnGetDirectoryPageNumber   = 'GetDirectoryPageNumber'
	m.OnGetNameAndLink           = 'GetNameAndLink'
	m.OnGetInfo                  = 'GetInfo'
	m.OnGetPageNumber            = 'GetPageNumber'
	m.OnGetImageURL              = 'GetImageURL'
	m.SortedList                 = true
end

function GetDirectoryPageNumber()
	if HTTP.GET(MODULE.RootURL .. '/albums-index-page-1') then
		PAGENUMBER = tonumber(CreateTXQuery(HTTP.Document).XPathString('//div[@class="f_left paginator"]/a[last()]')) or 1
		return no_error
	else
		return net_problem
	end
end

function GetNameAndLink()
	if HTTP.GET(MODULE.RootURL .. '/albums-index-page-' .. (URL + 1)) then
		CreateTXQuery(HTTP.Document).XPathHREFTitleAll('//div[@class="pic_box"]/a', LINKS, NAMES)
		return no_error
	else
		return net_problem
	end
end

function GetInfo()
	MANGAINFO.URL = MaybeFillHost(MODULE.RootURL, URL)
	if HTTP.GET(MANGAINFO.URL) then
		local x = CreateTXQuery(HTTP.Document)
		MANGAINFO.Title     = x.XPathString('//h2')
		MANGAINFO.CoverLink = x.XPathString('//div[@class="asTBcell uwthumb"]/img/@src')
		if MANGAINFO.CoverLink:match('^//') ~= nil then
			MANGAINFO.CoverLink = 'https:' .. MANGAINFO.CoverLink
		end
		MANGAINFO.Genres    = x.XPathStringAll('//div[@class="asTBcell uwconn"]/div[@class="addtags"]/a[not(@onclick)]')
		MANGAINFO.Summary   = x.XPathString('//div[@class="asTBcell uwconn"]/p'):gsub('簡介：', '')

		MANGAINFO.ChapterLinks.Add(x.XPathString('(//div[@class="pic_box tb"])[1]/a/@href'))
		MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
		CreateTXQuery(HTTP.Document).XPathStringAll('//select[@class="pageselect"]/option/@value', TASK.PageContainerLinks)
		TASK.PageNumber = TASK.PageContainerLinks.Count
		return true
	else
		return false
	end
end

function GetImageURL()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, 'photos-view-id-' .. TASK.PageContainerLinks[WORKID])) then
		TASK.PageLinks[WORKID] = MaybeFillHost(MODULE.RootURL, CreateTXQuery(HTTP.Document).XPathString('//img[@id="picarea"]/@src'))
		return true
	end
		return false
end
